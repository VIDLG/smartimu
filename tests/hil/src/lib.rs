use serialport::{SerialPortInfo, SerialPortType};
use smartimu::{
    DeviceEvent, DeviceMessage, DeviceResponse, ImuChipModel, ImuId, ProtocolErrorCode,
    RawImuSample, ResponseResult, SampleIndex, TimestampUs, WireMessage, decode_json,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::io::{ErrorKind, Read};
use std::time::{Duration, Instant};

const ESPRESSIF_USB_VID: u16 = 0x303a;
const DEFAULT_BAUD_RATE: u32 = 115_200;
const DEFAULT_DURATION: Duration = Duration::from_secs(10);
const DEFAULT_MIN_SAMPLES: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpectedImu {
    pub sensor_id: u16,
    pub label: &'static str,
    pub model: ImuChipModel,
}

pub const EXPECTED_IMUS: [ExpectedImu; 4] = [
    ExpectedImu {
        sensor_id: 1,
        label: "slot-1",
        model: ImuChipModel::Icm42688Hxy,
    },
    ExpectedImu {
        sensor_id: 2,
        label: "slot-2",
        model: ImuChipModel::Icm42688Pc,
    },
    ExpectedImu {
        sensor_id: 4,
        label: "slot-4",
        model: ImuChipModel::Qmi8658A,
    },
    ExpectedImu {
        sensor_id: 5,
        label: "slot-5",
        model: ImuChipModel::Sc7u22,
    },
];

#[derive(Clone, Debug)]
pub struct HilConfig {
    pub port: Option<String>,
    pub baud_rate: u32,
    pub duration: Duration,
    pub min_samples_per_imu: usize,
}

impl Default for HilConfig {
    fn default() -> Self {
        Self {
            port: None,
            baud_rate: DEFAULT_BAUD_RATE,
            duration: DEFAULT_DURATION,
            min_samples_per_imu: DEFAULT_MIN_SAMPLES,
        }
    }
}

impl HilConfig {
    pub fn from_env() -> Result<Self, String> {
        let port = non_empty_env("SMARTIMU_HIL_PORT").or_else(|| non_empty_env("ESPFLASH_PORT"));
        let duration = match non_empty_env("SMARTIMU_HIL_SECONDS") {
            Some(value) => Duration::from_secs(
                value
                    .parse::<u64>()
                    .map_err(|error| format!("invalid SMARTIMU_HIL_SECONDS={value:?}: {error}"))?,
            ),
            None => DEFAULT_DURATION,
        };
        let min_samples_per_imu = match non_empty_env("SMARTIMU_HIL_MIN_SAMPLES") {
            Some(value) => value
                .parse::<usize>()
                .map_err(|error| format!("invalid SMARTIMU_HIL_MIN_SAMPLES={value:?}: {error}"))?,
            None => DEFAULT_MIN_SAMPLES,
        };

        Ok(Self {
            port,
            duration,
            min_samples_per_imu,
            ..Self::default()
        })
    }
}

#[derive(Clone, Debug, Default)]
struct SensorObservation {
    model: Option<ImuChipModel>,
    samples: usize,
    orientations: usize,
    first_sample: Option<RawImuSample>,
    sample_changed: bool,
    last_sample_index: Option<SampleIndex>,
    last_sample_timestamp: Option<TimestampUs>,
}

#[derive(Clone, Debug)]
pub struct HilReport {
    pub port: String,
    pub protocol_messages: usize,
    pub json_decode_errors: usize,
    pub inventory_messages: usize,
    pub heartbeat_messages: usize,
    pub sensor_samples: BTreeMap<u16, usize>,
    pub sensor_orientations: BTreeMap<u16, usize>,
    pub sensor_data_not_ready: BTreeMap<u16, usize>,
}

#[derive(Debug, Default)]
struct HilMonitor {
    protocol_messages: usize,
    json_lines: usize,
    json_decode_errors: usize,
    first_decode_error: Option<String>,
    inventory_messages: usize,
    heartbeat_messages: usize,
    sensors: BTreeMap<u16, SensorObservation>,
    data_not_ready: BTreeMap<u16, usize>,
    failures: Vec<String>,
}

impl HilMonitor {
    fn observe_line(&mut self, line: &str) {
        let line = line.trim();
        if !line.starts_with('{') {
            return;
        }

        self.json_lines += 1;
        match decode_json(line) {
            Ok(message) => self.observe_message(message),
            Err(error) => {
                self.json_decode_errors += 1;
                if self.first_decode_error.is_none() {
                    self.first_decode_error = Some(format!("{error:?}: {}", abbreviate(line, 200)));
                }
            }
        }
    }

    fn observe_message(&mut self, message: WireMessage) {
        self.protocol_messages += 1;
        let WireMessage::DeviceMessage(message) = message else {
            self.failures
                .push(String::from("device emitted a HostRequest message"));
            return;
        };

        match message {
            DeviceMessage::Response(DeviceResponse::Inventory(response)) => match response.result {
                ResponseResult::Ok(inventory) => {
                    self.inventory_messages += 1;
                    for device in inventory.imu_devices {
                        let sensor_id = device.id.sensor_id.0;
                        if let Some(expected) = expected_imu(sensor_id) {
                            if device.chip_profile.model != expected.model {
                                self.failures.push(format!(
                                    "{} reported {:?}, expected {:?}",
                                    expected.label, device.chip_profile.model, expected.model
                                ));
                            }
                            if device.label.as_deref() != Some(expected.label) {
                                self.failures.push(format!(
                                    "sensor {sensor_id} label {:?}, expected {:?}",
                                    device.label, expected.label
                                ));
                            }
                            self.sensor(sensor_id).model = Some(device.chip_profile.model);
                        } else {
                            self.failures.push(format!(
                                "inventory contains unexpected active sensor {sensor_id}"
                            ));
                        }
                    }
                }
                ResponseResult::Err(error) => self
                    .failures
                    .push(format!("inventory response failed: {error:?}")),
            },
            DeviceMessage::Event(event) => self.observe_event(event),
            _ => {}
        }
    }

    fn observe_event(&mut self, event: DeviceEvent) {
        match event {
            DeviceEvent::ProbeDetected(event) => {
                let sensor_id = event.payload.imu_id.sensor_id.0;
                if let Some(expected) = expected_imu(sensor_id) {
                    let model = event.payload.chip_info.chip_profile.model;
                    if model != expected.model {
                        self.failures.push(format!(
                            "{} probe detected {:?}, expected {:?}",
                            expected.label, model, expected.model
                        ));
                    }
                    self.sensor(sensor_id).model = Some(model);
                } else {
                    self.failures
                        .push(format!("probe detected unexpected sensor {sensor_id}"));
                }
            }
            DeviceEvent::RawSample(event) => {
                self.observe_sample(
                    event.payload.imu_id,
                    event.payload.sample_index,
                    event.payload.timestamp_us,
                    event.payload.sample,
                );
            }
            DeviceEvent::Orientation(event) => {
                let sensor_id = event.payload.imu_id.sensor_id.0;
                if expected_imu(sensor_id).is_none() {
                    self.failures
                        .push(format!("orientation from unexpected sensor {sensor_id}"));
                    return;
                }
                if !quaternion_is_valid([
                    event.payload.quaternion.w,
                    event.payload.quaternion.x,
                    event.payload.quaternion.y,
                    event.payload.quaternion.z,
                ]) {
                    self.failures.push(format!(
                        "sensor {sensor_id} emitted a non-finite or non-normalized quaternion: {:?}",
                        event.payload.quaternion
                    ));
                }
                self.sensor(sensor_id).orientations += 1;
            }
            DeviceEvent::Heartbeat(event) => {
                self.heartbeat_messages += 1;
                let actual: BTreeSet<u16> = event
                    .payload
                    .active_imu_ids
                    .into_iter()
                    .map(|id| id.sensor_id.0)
                    .collect();
                let expected: BTreeSet<u16> =
                    EXPECTED_IMUS.iter().map(|imu| imu.sensor_id).collect();
                if actual != expected {
                    self.failures.push(format!(
                        "heartbeat active sensors {actual:?}, expected {expected:?}"
                    ));
                }
            }
            DeviceEvent::Error(event) => {
                let sensor_id = event.payload.imu_id.map(|id| id.sensor_id.0);
                let expected_disabled_slot =
                    sensor_id == Some(3) && event.payload.code == ProtocolErrorCode::ChipNotFound;
                let transient_data_not_ready = sensor_id.is_some_and(|sensor_id| {
                    expected_imu(sensor_id).is_some()
                        && event.payload.code == ProtocolErrorCode::DataNotReady
                });
                if transient_data_not_ready {
                    *self.data_not_ready.entry(sensor_id.unwrap()).or_default() += 1;
                } else if !expected_disabled_slot {
                    self.failures.push(format!(
                        "device error for sensor {sensor_id:?}: {:?}: {}",
                        event.payload.code, event.payload.details
                    ));
                }
            }
            DeviceEvent::Power(_) => {}
        }
    }

    fn observe_sample(
        &mut self,
        imu_id: ImuId,
        sample_index: SampleIndex,
        timestamp: TimestampUs,
        sample: RawImuSample,
    ) {
        let sensor_id = imu_id.sensor_id.0;
        if expected_imu(sensor_id).is_none() {
            self.failures
                .push(format!("sample from unexpected sensor {sensor_id}"));
            return;
        }

        let mut failures = Vec::new();
        {
            let observation = self.sensor(sensor_id);
            if let Some(previous) = observation.last_sample_index
                && !is_forward_u32(previous.0, sample_index.0)
            {
                failures.push(format!(
                    "sensor {sensor_id} sample index did not advance: {} -> {}",
                    previous.0, sample_index.0
                ));
            }
            if let Some(previous) = observation.last_sample_timestamp
                && timestamp.0 <= previous.0
            {
                failures.push(format!(
                    "sensor {sensor_id} sample timestamp did not advance: {} -> {}",
                    previous.0, timestamp.0
                ));
            }

            match observation.first_sample {
                Some(first) if first != sample => observation.sample_changed = true,
                None => observation.first_sample = Some(sample),
                _ => {}
            }
            observation.samples += 1;
            observation.last_sample_index = Some(sample_index);
            observation.last_sample_timestamp = Some(timestamp);
        }
        self.failures.extend(failures);
    }

    fn sensor(&mut self, sensor_id: u16) -> &mut SensorObservation {
        self.sensors.entry(sensor_id).or_default()
    }

    fn finish(self, port: String, min_samples: usize) -> Result<HilReport, String> {
        let mut failures = self.failures;
        if self.protocol_messages == 0 {
            if self.json_lines > 0 {
                failures.push(format!(
                    "received {} JSON-looking lines but none matched the current SmartIMU protocol; first decode error: {}",
                    self.json_lines,
                    self.first_decode_error.as_deref().unwrap_or("unknown")
                ));
            } else {
                failures.push(String::from("received no SmartIMU protocol messages"));
            }
        }
        if self.inventory_messages == 0
            && !EXPECTED_IMUS.iter().all(|expected| {
                self.sensors
                    .get(&expected.sensor_id)
                    .and_then(|sensor| sensor.model)
                    == Some(expected.model)
            })
        {
            failures.push(String::from(
                "did not receive a valid inventory or probe result for every expected IMU",
            ));
        }
        if self.heartbeat_messages == 0 {
            failures.push(String::from("received no heartbeat"));
        }

        for expected in EXPECTED_IMUS {
            match self.sensors.get(&expected.sensor_id) {
                Some(sensor) => {
                    if sensor.model != Some(expected.model) {
                        failures.push(format!(
                            "{} model was not confirmed as {:?}",
                            expected.label, expected.model
                        ));
                    }
                    if sensor.samples < min_samples {
                        failures.push(format!(
                            "{} produced {} samples, expected at least {min_samples}",
                            expected.label, sensor.samples
                        ));
                    }
                    if sensor.orientations < min_samples {
                        failures.push(format!(
                            "{} produced {} orientations, expected at least {min_samples}",
                            expected.label, sensor.orientations
                        ));
                    }
                    if sensor.samples >= min_samples && !sensor.sample_changed {
                        failures.push(format!(
                            "{} raw accel/gyro stayed bit-for-bit constant across {} samples",
                            expected.label, sensor.samples
                        ));
                    }
                }
                None => failures.push(format!("received no data for {}", expected.label)),
            }
        }

        if self.json_decode_errors > 0 {
            failures.push(format!(
                "{} JSON protocol lines failed to decode; first error: {}",
                self.json_decode_errors,
                self.first_decode_error.as_deref().unwrap_or("unknown")
            ));
        }

        if !failures.is_empty() {
            let mut message = format!("SmartIMU HIL failed on {port}:");
            for failure in failures {
                let _ = write!(message, "\n- {failure}");
            }
            return Err(message);
        }

        Ok(HilReport {
            port,
            protocol_messages: self.protocol_messages,
            json_decode_errors: self.json_decode_errors,
            inventory_messages: self.inventory_messages,
            heartbeat_messages: self.heartbeat_messages,
            sensor_samples: EXPECTED_IMUS
                .iter()
                .map(|expected| {
                    (
                        expected.sensor_id,
                        self.sensors
                            .get(&expected.sensor_id)
                            .map_or(0, |sensor| sensor.samples),
                    )
                })
                .collect(),
            sensor_orientations: EXPECTED_IMUS
                .iter()
                .map(|expected| {
                    (
                        expected.sensor_id,
                        self.sensors
                            .get(&expected.sensor_id)
                            .map_or(0, |sensor| sensor.orientations),
                    )
                })
                .collect(),
            sensor_data_not_ready: EXPECTED_IMUS
                .iter()
                .map(|expected| {
                    (
                        expected.sensor_id,
                        self.data_not_ready
                            .get(&expected.sensor_id)
                            .copied()
                            .unwrap_or_default(),
                    )
                })
                .collect(),
        })
    }
}

pub fn run(config: HilConfig) -> Result<HilReport, String> {
    let port_name = match config.port.clone() {
        Some(port) => port,
        None => detect_serial_port()?,
    };
    println!(
        "SmartIMU HIL: reading {} at {} baud for {:?}",
        port_name, config.baud_rate, config.duration
    );

    let mut port = serialport::new(&port_name, config.baud_rate)
        .timeout(Duration::from_millis(200))
        .open()
        .map_err(|error| format!("failed to open {port_name}: {error}"))?;

    let deadline = Instant::now() + config.duration;
    let mut monitor = HilMonitor::default();
    let mut pending = Vec::with_capacity(2048);
    let mut chunk = [0u8; 512];

    while Instant::now() < deadline {
        match port.read(&mut chunk) {
            Ok(0) => {}
            Ok(read) => {
                pending.extend_from_slice(&chunk[..read]);
                process_complete_lines(&mut pending, &mut monitor);
                if pending.len() > 8192 {
                    pending.clear();
                    monitor
                        .failures
                        .push(String::from("serial line exceeded 8192 bytes"));
                }
            }
            Err(error) if matches!(error.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) => {}
            Err(error) => return Err(format!("failed while reading {port_name}: {error}")),
        }
    }

    monitor.finish(port_name, config.min_samples_per_imu)
}

pub fn detect_serial_port() -> Result<String, String> {
    select_serial_port(
        serialport::available_ports()
            .map_err(|error| format!("failed to enumerate serial ports: {error}"))?,
    )
}

fn select_serial_port(ports: Vec<SerialPortInfo>) -> Result<String, String> {
    let espressif: Vec<&SerialPortInfo> = ports
        .iter()
        .filter(|port| {
            matches!(
                &port.port_type,
                SerialPortType::UsbPort(info) if info.vid == ESPRESSIF_USB_VID
            )
        })
        .collect();

    if espressif.len() == 1 {
        return Ok(espressif[0].port_name.clone());
    }
    if ports.len() == 1 {
        return Ok(ports[0].port_name.clone());
    }

    let available = if ports.is_empty() {
        String::from("none")
    } else {
        ports
            .iter()
            .map(|port| port.port_name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    Err(format!(
        "could not select one serial port (available: {available}); set SMARTIMU_HIL_PORT or ESPFLASH_PORT"
    ))
}

fn process_complete_lines(pending: &mut Vec<u8>, monitor: &mut HilMonitor) {
    while let Some(newline) = pending.iter().position(|byte| *byte == b'\n') {
        let line: Vec<u8> = pending.drain(..=newline).collect();
        let line = String::from_utf8_lossy(&line);
        monitor.observe_line(&line);
    }
}

fn expected_imu(sensor_id: u16) -> Option<&'static ExpectedImu> {
    EXPECTED_IMUS
        .iter()
        .find(|expected| expected.sensor_id == sensor_id)
}

fn is_forward_u32(previous: u32, current: u32) -> bool {
    let delta = current.wrapping_sub(previous);
    delta != 0 && delta < (1 << 31)
}

fn quaternion_is_valid(quaternion: [f32; 4]) -> bool {
    if !quaternion.iter().all(|component| component.is_finite()) {
        return false;
    }
    let norm_squared = quaternion
        .iter()
        .map(|component| component * component)
        .sum::<f32>();
    (0.9..=1.1).contains(&norm_squared)
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn abbreviate(value: &str, max_chars: usize) -> String {
    let mut abbreviated: String = value.chars().take(max_chars).collect();
    if value.chars().count() > max_chars {
        abbreviated.push_str("...");
    }
    abbreviated
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
