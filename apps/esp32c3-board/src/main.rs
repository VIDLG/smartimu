#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

extern crate alloc;

mod board;

use alloc::string::String;
use alloc::vec::Vec;
use embassy_executor::Spawner;
use embassy_time::{Instant, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::spi::Mode;
use esp_hal::spi::master::{Config, Spi};
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::usb_serial_jtag::UsbSerialJtag;
use esp_println::println;
use smartimu::EspImuBus;
use smartimu::fusion::{FusionFilter, FusionFilterSettings};
use smartimu::{
    BusInfo, ImuBus, ImuChip, ImuChipProfile, ImuDriver, ImuNodeInfo, ImuSampleConfig,
    OrientationFrame, ProbePlan, RangeDps, RangeG, SampleRateHz, SessionRuntime, SpiProfile,
    Turnaround, WireFormat, bounded_string, encode_binary_packet, encode_json, probe,
};

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    esp_println::println!("panic: {}", info);
    loop {
        unsafe { core::arch::asm!("wfi") }
    }
}

#[derive(Clone)]
struct DetectedImu {
    name: &'static str,
    driver: &'static dyn ImuDriver,
    chip_profile: ImuChipProfile,
    profile: SpiProfile,
    sample_config: ImuSampleConfig,
}

struct ImuRuntime {
    config: &'static board::BoardImuConfig,
    detected: Option<DetectedImu>,
    sample_index: u32,
    fusion: Option<FusionFilter>,
    last_orientation_timestamp_us: Option<u64>,
}

impl ImuRuntime {
    const fn new(config: &'static board::BoardImuConfig) -> Self {
        Self {
            config,
            detected: None,
            sample_index: 0,
            fusion: None,
            last_orientation_timestamp_us: None,
        }
    }
}

struct Transport<'d> {
    usb: Option<UsbSerialJtag<'d, esp_hal::Blocking>>,
    mode: board::TransportMode,
}

impl<'d> Transport<'d> {
    fn new(usb: Option<UsbSerialJtag<'d, esp_hal::Blocking>>, mode: board::TransportMode) -> Self {
        Self { usb, mode }
    }

    fn format(&self) -> WireFormat {
        match self.mode {
            board::TransportMode::Json => WireFormat::Json,
            board::TransportMode::Binary => WireFormat::Binary,
        }
    }

    fn emit_frame(&mut self, frame: &smartimu::DeviceFrame) {
        let wire_frame = smartimu::WireFrame::Device(frame.clone());
        match self.mode {
            board::TransportMode::Json => match encode_json::<768>(&wire_frame) {
                Ok(line) => {
                    println!("{}", line);
                }
                Err(_) => {}
            },
            board::TransportMode::Binary => match encode_binary_packet::<1024>(&wire_frame) {
                Ok(packet) => {
                    let _ = self.write_all(packet.as_slice());
                }
                Err(_) => {}
            },
        }
    }

    fn write_all(&mut self, mut data: &[u8]) -> Result<(), ()> {
        let Some(usb) = self.usb.as_mut() else {
            return Err(());
        };
        while !data.is_empty() {
            usb.write(data).map_err(|_| ())?;
            usb.flush_tx().map_err(|_| ())?;
            data = &[];
        }
        Ok(())
    }
}

#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    init_heap();

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt =
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

    let spi_config = Config::default()
        .with_frequency(Rate::from_khz(board::SPI_FREQ_KHZ))
        .with_mode(Mode::_0);
    let mut spi = Spi::new(peripherals.SPI2, spi_config)
        .unwrap()
        .with_sck(peripherals.GPIO6)
        .with_mosi(peripherals.GPIO7)
        .with_miso(peripherals.GPIO2);

    let mut bus = EspImuBus::new(&mut spi)
        .with_target(
            board::BOARD_IMUS[0].target,
            Output::new(peripherals.GPIO8, Level::High, OutputConfig::default()),
        )
        .with_target(
            board::BOARD_IMUS[1].target,
            Output::new(peripherals.GPIO4, Level::High, OutputConfig::default()),
        )
        .with_target(
            board::BOARD_IMUS[3].target,
            Output::new(peripherals.GPIO5, Level::High, OutputConfig::default()),
        )
        .with_target(
            board::BOARD_IMUS[4].target,
            Output::new(peripherals.GPIO1, Level::High, OutputConfig::default()),
        );
    let usb = match board::TRANSPORT_MODE {
        board::TransportMode::Json => None,
        board::TransportMode::Binary => Some(UsbSerialJtag::new(peripherals.USB_DEVICE)),
    };
    let mut transport = Transport::new(usb, board::TRANSPORT_MODE);
    let boot = Instant::now();
    let mut session = SessionRuntime::new(board::SYSTEM_ID, 1, transport.format());

    let mut runtimes = board::BOARD_IMUS
        .each_ref()
        .map(|config| ImuRuntime::new(config));
    let mut heartbeat_count: u32 = 0;

    Timer::after_millis(board::POWER_UP_DELAY_MS).await;

    transport.emit_frame(&session.ping(emit_timestamp_us(boot), "ping"));

    for runtime in &mut runtimes {
        match probe(
            &mut bus,
            runtime.config.target,
            ProbePlan::Auto {
                candidates: runtime.config.candidates,
            },
        )
        .await
        {
            Ok(Some(probe_match)) => {
                let driver = probe_match.driver;
                let profile = probe_match.profile;
                let chip_profile = probe_match.info.chip_profile.clone();
                let sample_config = select_imu_sample_config(&chip_profile);
                let result = match driver.reset(&mut bus, runtime.config.target).await {
                    Ok(()) => {
                        driver
                            .configure(&mut bus, runtime.config.target, &sample_config)
                            .await
                    }
                    Err(error) => Err(error),
                };

                match result {
                    Ok(()) => {
                        runtime.detected = Some(DetectedImu {
                            name: runtime
                                .config
                                .candidates
                                .iter()
                                .find(|candidate| core::ptr::eq(candidate.info.driver, driver))
                                .map(|candidate| candidate.info.name)
                                .unwrap_or("unknown"),
                            driver,
                            chip_profile: chip_profile.clone(),
                            profile,
                            sample_config,
                        });
                        let mut fusion_settings = FusionFilterSettings::default();
                        fusion_settings.gyroscope_range_dps = sample_config.gyro_range.0 as f32;
                        runtime.fusion = Some(FusionFilter::new(fusion_settings));
                        runtime.last_orientation_timestamp_us = None;
                        transport.emit_frame(&session.probe_result(
                            emit_timestamp_us(boot),
                            runtime.config.imu_id,
                            runtime.detected.as_ref().unwrap().name,
                            smartimu::ProbeResult::Detected {
                                driver_id: smartimu::bounded_string(
                                    runtime.detected.as_ref().unwrap().name,
                                    smartimu::MAX_LABEL_LEN,
                                ),
                                chip: chip_profile.chip,
                                profile,
                            },
                        ));
                        if chip_profile.chip != runtime.config.expected {
                            transport.emit_frame(&session.error(
                                emit_timestamp_us(boot),
                                Some(runtime.config.imu_id),
                                smartimu::SmartImuError::ChipNotFound,
                                "detected chip mismatches expected chip",
                            ));
                        }
                    }
                    Err(error) => {
                        transport.emit_frame(&session.probe_result(
                            emit_timestamp_us(boot),
                            runtime.config.imu_id,
                            "probe-match",
                            smartimu::ProbeResult::Failed { error },
                        ));
                    }
                }
            }
            Ok(None) => {
                transport.emit_frame(&session.probe_result(
                    emit_timestamp_us(boot),
                    runtime.config.imu_id,
                    "none",
                    smartimu::ProbeResult::NotDetected,
                ));
                transport.emit_frame(&session.error(
                    emit_timestamp_us(boot),
                    Some(runtime.config.imu_id),
                    smartimu::SmartImuError::ChipNotFound,
                    &probe_snapshot(&mut bus, runtime.config.target, runtime.config.expected),
                ));
            }
            Err(error) => {
                transport.emit_frame(&session.error(
                    emit_timestamp_us(boot),
                    Some(runtime.config.imu_id),
                    error,
                    "probe error",
                ));
            }
        }
    }

    transport.emit_frame(&session.inventory(
        emit_timestamp_us(boot),
        "esp32c3-board",
        bus_infos(),
        imu_infos(&runtimes),
    ));

    loop {
        let mut active_imus = 0u16;
        for runtime in &mut runtimes {
            let Some(detected) = runtime.detected.as_ref() else {
                continue;
            };
            active_imus += 1;

            if let Err(error) = bus.apply_profile(runtime.config.target, detected.profile) {
                transport.emit_frame(&session.error(
                    emit_timestamp_us(boot),
                    Some(runtime.config.imu_id),
                    error,
                    "profile switch failed",
                ));
                continue;
            }

            match detected
                .driver
                .read_sample(
                    &mut bus,
                    runtime.config.target,
                    smartimu::SampleReadoutRequest::default(),
                )
                .await
            {
                Ok(raw) => {
                    runtime.sample_index = runtime.sample_index.wrapping_add(1);
                    let sample_timestamp_us = Instant::now().duration_since(boot).as_micros();
                    transport.emit_frame(&session.sample(
                        emit_timestamp_us(boot),
                        runtime.config.imu_id,
                        detected.chip_profile.chip,
                        runtime.sample_index,
                        sample_timestamp_us,
                        raw,
                        0x0001,
                    ));

                    let scale = smartimu::Imu6Scale::from(detected.sample_config);
                    if let Some(fusion) = runtime.fusion.as_mut() {
                        let physical = raw.imu6.to_physical(scale);
                        let accel_ms2 = [
                            physical.accel_g[0] * 9.81,
                            physical.accel_g[1] * 9.81,
                            physical.accel_g[2] * 9.81,
                        ];
                        let gyro_rads = [
                            physical.gyro_dps[0].to_radians(),
                            physical.gyro_dps[1].to_radians(),
                            physical.gyro_dps[2].to_radians(),
                        ];
                        let dt_s = if let Some(last) = runtime.last_orientation_timestamp_us {
                            ((sample_timestamp_us.saturating_sub(last)) as f32 / 1_000_000.0)
                                .clamp(0.0, 0.1)
                        } else {
                            0.0
                        };
                        runtime.last_orientation_timestamp_us = Some(sample_timestamp_us);
                        let quaternion = fusion.update_imu(accel_ms2, gyro_rads, dt_s);
                        transport.emit_frame(&smartimu::DeviceFrame::Orientation(
                            OrientationFrame {
                                header: session.header(emit_timestamp_us(boot)),
                                imu_id: runtime.config.imu_id,
                                imu_chip: detected.chip_profile.chip,
                                sample_index: runtime.sample_index,
                                sample_timestamp_us,
                                quaternion,
                            },
                        ));
                    }
                }
                Err(error) => {
                    transport.emit_frame(&session.error(
                        emit_timestamp_us(boot),
                        Some(runtime.config.imu_id),
                        error,
                        "sample error",
                    ));
                }
            }
        }

        transport.emit_frame(&session.heartbeat(emit_timestamp_us(boot), active_imus));
        heartbeat_count = heartbeat_count.wrapping_add(1);
        if heartbeat_count % 20 == 0 {
            transport.emit_frame(&session.ping(emit_timestamp_us(boot), "ping"));
            transport.emit_frame(&session.inventory(
                emit_timestamp_us(boot),
                "esp32c3-board",
                bus_infos(),
                imu_infos(&runtimes),
            ));
        }

        Timer::after_millis(board::STREAM_INTERVAL_MS).await;
    }
}

fn init_heap() {
    esp_alloc::heap_allocator!(size: 32 * 1024);
}

fn emit_timestamp_us(boot: Instant) -> u64 {
    Instant::now().duration_since(boot).as_micros()
}

fn bus_infos() -> Vec<BusInfo> {
    let mut buses = Vec::new();
    buses.push(BusInfo {
        bus_id: board::BUS_ID,
        label: bounded_string("spi2", smartimu::MAX_LABEL_LEN),
    });
    buses
}

fn imu_infos(runtimes: &[ImuRuntime; 5]) -> Vec<ImuNodeInfo> {
    let mut imus = Vec::new();
    for runtime in runtimes {
        let Some(detected) = runtime.detected.as_ref() else {
            continue;
        };

        let info = ImuNodeInfo {
            id: runtime.config.imu_id,
            bus_id: board::BUS_ID,
            chip_profile: detected.chip_profile.clone(),
            label: Some(bounded_string(
                runtime.config.label,
                smartimu::MAX_LABEL_LEN,
            )),
            sample_config: detected.sample_config,
        };
        imus.push(info);
    }
    imus
}

fn select_imu_sample_config(chip_profile: &ImuChipProfile) -> ImuSampleConfig {
    let sample_config_options = &chip_profile.sample_config_options;
    let preferred = ImuSampleConfig {
        accel_range: RangeG(8),
        gyro_range: RangeDps(500),
        sample_rate_hz: SampleRateHz(100),
    };

    sample_config_options
        .contains(&preferred)
        .then_some(preferred)
        .or_else(|| sample_config_options.first_config())
        .unwrap_or(preferred)
}

fn probe_snapshot(
    bus: &mut dyn ImuBus,
    target: smartimu::ImuTargetId,
    _expected: ImuChip,
) -> String {
    let _ = bus.apply_profile(target, board::PROFILE_MODE0);
    let r00_m0 = bus
        .read_reg(target, 0x00, Turnaround(0))
        .ok()
        .unwrap_or(0xFF);
    let r01_m0 = bus
        .read_reg(target, 0x01, Turnaround(0))
        .ok()
        .unwrap_or(0xFF);
    let r75_m0 = bus
        .read_reg(target, 0x75, Turnaround(0))
        .ok()
        .unwrap_or(0xFF);

    let _ = bus.apply_profile(target, board::PROFILE_MODE3);
    let r00_m3 = bus
        .read_reg(target, 0x00, Turnaround(0))
        .ok()
        .unwrap_or(0xFF);
    let r01_m3 = bus
        .read_reg(target, 0x01, Turnaround(0))
        .ok()
        .unwrap_or(0xFF);
    let r75_m3 = bus
        .read_reg(target, 0x75, Turnaround(0))
        .ok()
        .unwrap_or(0xFF);

    let mut out = String::new();
    let _ = core::fmt::write(
        &mut out,
        format_args!(
            "m0 r00={:02X} r01={:02X} r75={:02X} m3 r00={:02X} r01={:02X} r75={:02X}",
            r00_m0, r01_m0, r75_m0, r00_m3, r01_m3, r75_m3
        ),
    );
    out
}
