use super::*;
use serialport::SerialPortType;
use smartimu::{RawImu6, RawImuSample};

#[test]
fn forward_index_accepts_normal_progress_and_wraparound() {
    assert!(is_forward_u32(10, 11));
    assert!(is_forward_u32(u32::MAX, 0));
    assert!(!is_forward_u32(10, 10));
    assert!(!is_forward_u32(11, 10));
}

#[test]
fn quaternion_requires_finite_near_unit_values() {
    assert!(quaternion_is_valid([1.0, 0.0, 0.0, 0.0]));
    assert!(quaternion_is_valid([0.5, 0.5, 0.5, 0.5]));
    assert!(!quaternion_is_valid([f32::NAN, 0.0, 0.0, 0.0]));
    assert!(!quaternion_is_valid([2.0, 0.0, 0.0, 0.0]));
}

#[test]
fn sample_observation_detects_progress_and_data_changes() {
    let mut monitor = HilMonitor::default();
    monitor.observe_sample(
        imu_id(1),
        SampleIndex(7),
        TimestampUs(1_000),
        sample([1, 2, 3], [4, 5, 6]),
    );
    monitor.observe_sample(
        imu_id(1),
        SampleIndex(8),
        TimestampUs(2_000),
        sample([1, 2, 4], [4, 5, 6]),
    );

    let sensor = monitor.sensors.get(&1).unwrap();
    assert_eq!(sensor.samples, 2);
    assert!(sensor.sample_changed);
    assert!(monitor.failures.is_empty());
}

#[test]
fn sample_observation_rejects_duplicate_index_and_timestamp() {
    let mut monitor = HilMonitor::default();
    let sample = sample([1, 2, 3], [4, 5, 6]);
    monitor.observe_sample(imu_id(1), SampleIndex(7), TimestampUs(1_000), sample);
    monitor.observe_sample(imu_id(1), SampleIndex(7), TimestampUs(1_000), sample);

    assert_eq!(monitor.failures.len(), 2);
    assert!(monitor.failures[0].contains("sample index did not advance"));
    assert!(monitor.failures[1].contains("sample timestamp did not advance"));
}

#[test]
fn transient_data_not_ready_is_counted_without_failing() {
    let mut monitor = HilMonitor::default();
    let mut session = smartimu::DeviceSession::new(smartimu::SystemId(1), smartimu::SessionId(1));
    monitor.observe_message(WireMessage::DeviceMessage(session.error(
        TimestampUs(1_000),
        Some(imu_id(1)),
        smartimu::SmartImuError::DataNotReady,
    )));

    assert_eq!(monitor.data_not_ready.get(&1), Some(&1));
    assert!(monitor.failures.is_empty());
}

#[test]
fn communication_error_still_fails() {
    let mut monitor = HilMonitor::default();
    let mut session = smartimu::DeviceSession::new(smartimu::SystemId(1), smartimu::SessionId(1));
    monitor.observe_message(WireMessage::DeviceMessage(session.error(
        TimestampUs(1_000),
        Some(imu_id(1)),
        smartimu::SmartImuError::CommunicationError,
    )));

    assert_eq!(monitor.failures.len(), 1);
    assert!(monitor.failures[0].contains("CommunicationError"));
}

#[test]
fn final_validation_rejects_a_stuck_sensor() {
    let mut monitor = passing_monitor();
    monitor.sensors.get_mut(&4).unwrap().sample_changed = false;

    let error = monitor.finish(String::from("COM5"), 5).unwrap_err();
    assert!(error.contains("slot-4 raw accel/gyro stayed bit-for-bit constant"));
}

#[test]
fn old_protocol_json_is_reported_as_a_firmware_mismatch() {
    let mut monitor = HilMonitor::default();
    monitor.observe_line(
        r#"{"Device":{"Heartbeat":{"active_imus":4,"header":{"protocol_version":1}}}}"#,
    );

    let error = monitor.finish(String::from("COM5"), 5).unwrap_err();
    assert!(error.contains("none matched the current SmartIMU protocol"));
}

#[test]
fn one_available_serial_port_is_selected() {
    let port = SerialPortInfo {
        port_name: String::from("COM5"),
        port_type: SerialPortType::Unknown,
    };

    assert_eq!(select_serial_port(vec![port]).unwrap(), "COM5");
}

#[test]
fn ambiguous_serial_ports_require_an_explicit_selection() {
    let ports = vec![
        SerialPortInfo {
            port_name: String::from("COM5"),
            port_type: SerialPortType::Unknown,
        },
        SerialPortInfo {
            port_name: String::from("COM6"),
            port_type: SerialPortType::Unknown,
        },
    ];

    let error = select_serial_port(ports).unwrap_err();
    assert!(error.contains("SMARTIMU_HIL_PORT"));
}

fn passing_monitor() -> HilMonitor {
    let mut monitor = HilMonitor {
        protocol_messages: 1,
        heartbeat_messages: 1,
        ..HilMonitor::default()
    };
    for expected in EXPECTED_IMUS {
        monitor.sensors.insert(
            expected.sensor_id,
            SensorObservation {
                model: Some(expected.model),
                samples: 5,
                orientations: 5,
                first_sample: Some(sample([1, 2, 3], [4, 5, 6])),
                sample_changed: true,
                last_sample_index: Some(SampleIndex(5)),
                last_sample_timestamp: Some(TimestampUs(5_000)),
            },
        );
    }
    monitor
}

fn imu_id(sensor_id: u16) -> ImuId {
    ImuId {
        system_id: smartimu::SystemId(1),
        sensor_id: smartimu::SensorId(sensor_id),
    }
}

fn sample(accel: [i16; 3], gyro: [i16; 3]) -> RawImuSample {
    RawImuSample {
        imu6: RawImu6 { accel, gyro },
        temperature: None,
        sensor_timestamp: None,
    }
}
