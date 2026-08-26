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
    BinaryEncoder, BusInfo, DeviceEvent, DeviceMessage, DeviceSession, ImuBus, ImuChipProfile,
    ImuDeviceInfo, ImuDriver, ImuSampleConfig, OrientationEvent, OrientationPayload, ProbePlan,
    RangeDps, RangeG, SampleIndex, SampleRateHz, SessionId, SpiProfile, TimestampUs, encode_json,
    probe,
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
    sample_index: SampleIndex,
    fusion: Option<FusionFilter>,
    last_orientation_timestamp_us: Option<TimestampUs>,
}

impl ImuRuntime {
    const fn new(config: &'static board::BoardImuConfig) -> Self {
        Self {
            config,
            detected: None,
            sample_index: SampleIndex(0),
            fusion: None,
            last_orientation_timestamp_us: None,
        }
    }
}

struct Transport<'d> {
    usb: Option<UsbSerialJtag<'d, esp_hal::Blocking>>,
    mode: board::TransportMode,
    binary_encoder: BinaryEncoder,
}

impl<'d> Transport<'d> {
    fn new(usb: Option<UsbSerialJtag<'d, esp_hal::Blocking>>, mode: board::TransportMode) -> Self {
        Self {
            usb,
            mode,
            binary_encoder: BinaryEncoder::new(),
        }
    }

    fn emit_message(&mut self, message: &smartimu::DeviceMessage) {
        let wire_message = smartimu::WireMessage::DeviceMessage(message.clone());
        match self.mode {
            board::TransportMode::Json => {
                match encode_json::<{ board::JSON_MESSAGE_BUFFER_LEN }>(&wire_message) {
                    Ok(line) => {
                        println!("{}", line);
                    }
                    Err(_) => {}
                }
            }
            board::TransportMode::Binary => {
                let Some(usb) = self.usb.as_mut() else {
                    return;
                };
                match self.binary_encoder.encode_packet(&wire_message) {
                    Ok(packet) => {
                        let _ = write_all(usb, packet);
                    }
                    Err(_) => {}
                }
            }
        }
    }
}

fn write_all(usb: &mut UsbSerialJtag<'_, esp_hal::Blocking>, mut data: &[u8]) -> Result<(), ()> {
    while !data.is_empty() {
        usb.write(data).map_err(|_| ())?;
        usb.flush_tx().map_err(|_| ())?;
        data = &[];
    }
    Ok(())
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
    let mut session = DeviceSession::new(board::SYSTEM_ID, SessionId(1));

    let mut runtimes = board::BOARD_IMUS
        .each_ref()
        .map(|config| ImuRuntime::new(config));
    let mut heartbeat_count: u32 = 0;

    Timer::after_millis(board::POWER_UP_DELAY_MS).await;

    transport.emit_message(&session.pong(device_timestamp_us(boot), "pong"));

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
                        transport.emit_message(&session.probe_detected(
                            device_timestamp_us(boot),
                            runtime.config.imu_id,
                            runtime.detected.as_ref().unwrap().name,
                            profile,
                            probe_match.info.clone(),
                        ));
                        if chip_profile.model != runtime.config.expected {
                            transport.emit_message(&session.error(
                                device_timestamp_us(boot),
                                Some(runtime.config.imu_id),
                                smartimu::SmartImuError::ChipNotFound,
                            ));
                        }
                    }
                    Err(error) => {
                        transport.emit_message(&session.error(
                            device_timestamp_us(boot),
                            Some(runtime.config.imu_id),
                            error,
                        ));
                    }
                }
            }
            Ok(None) => {
                transport.emit_message(&session.error(
                    device_timestamp_us(boot),
                    Some(runtime.config.imu_id),
                    smartimu::SmartImuError::ChipNotFound,
                ));
            }
            Err(error) => {
                transport.emit_message(&session.error(
                    device_timestamp_us(boot),
                    Some(runtime.config.imu_id),
                    error,
                ));
            }
        }
    }

    transport.emit_message(&session.inventory_response(
        device_timestamp_us(boot),
        "esp32c3-board",
        bus_infos(),
        imu_device_infos(&runtimes),
    ));

    loop {
        let mut active_imu_ids = Vec::new();
        for runtime in &mut runtimes {
            let Some(detected) = runtime.detected.as_ref() else {
                continue;
            };
            active_imu_ids.push(runtime.config.imu_id);

            if let Err(error) = bus.apply_profile(runtime.config.target, detected.profile) {
                transport.emit_message(&session.error(
                    device_timestamp_us(boot),
                    Some(runtime.config.imu_id),
                    error,
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
                    runtime.sample_index = runtime.sample_index.wrapping_next();
                    let sample_timestamp_us =
                        TimestampUs(Instant::now().duration_since(boot).as_micros());
                    transport.emit_message(&session.raw_sample(
                        device_timestamp_us(boot),
                        runtime.config.imu_id,
                        runtime.sample_index,
                        sample_timestamp_us,
                        raw,
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
                            (sample_timestamp_us.elapsed_since(last) as f32 / 1_000_000.0)
                                .clamp(0.0, 0.1)
                        } else {
                            0.0
                        };
                        runtime.last_orientation_timestamp_us = Some(sample_timestamp_us);
                        let quaternion = fusion.update_imu(accel_ms2, gyro_rads, dt_s);
                        transport.emit_message(&DeviceMessage::Event(DeviceEvent::Orientation(
                            OrientationEvent {
                                header: session.header(device_timestamp_us(boot)),
                                payload: OrientationPayload {
                                    imu_id: runtime.config.imu_id,
                                    sample_index: runtime.sample_index,
                                    timestamp_us: sample_timestamp_us,
                                    quaternion,
                                },
                            },
                        )));
                    }
                }
                Err(error) => {
                    transport.emit_message(&session.error(
                        device_timestamp_us(boot),
                        Some(runtime.config.imu_id),
                        error,
                    ));
                }
            }
        }

        transport.emit_message(&session.heartbeat(device_timestamp_us(boot), active_imu_ids));
        heartbeat_count = heartbeat_count.wrapping_add(1);
        if heartbeat_count % 20 == 0 {
            transport.emit_message(&session.pong(device_timestamp_us(boot), "pong"));
            transport.emit_message(&session.inventory_response(
                device_timestamp_us(boot),
                "esp32c3-board",
                bus_infos(),
                imu_device_infos(&runtimes),
            ));
        }

        Timer::after_millis(board::STREAM_INTERVAL_MS).await;
    }
}

fn init_heap() {
    esp_alloc::heap_allocator!(size: 32 * 1024);
}

fn device_timestamp_us(boot: Instant) -> TimestampUs {
    TimestampUs(Instant::now().duration_since(boot).as_micros())
}

fn bus_infos() -> Vec<BusInfo> {
    let mut buses = Vec::new();
    buses.push(BusInfo {
        bus_id: board::BUS_ID,
        label: String::from("spi2"),
    });
    buses
}

fn imu_device_infos(runtimes: &[ImuRuntime; 5]) -> Vec<ImuDeviceInfo> {
    let mut imu_devices = Vec::new();
    for runtime in runtimes {
        let Some(detected) = runtime.detected.as_ref() else {
            continue;
        };

        let info = ImuDeviceInfo {
            id: runtime.config.imu_id,
            bus_id: board::BUS_ID,
            chip_profile: detected.chip_profile.clone(),
            label: Some(String::from(runtime.config.label)),
            sample_config: detected.sample_config,
        };
        imu_devices.push(info);
    }
    imu_devices
}

fn select_imu_sample_config(chip_profile: &ImuChipProfile) -> ImuSampleConfig {
    let sample_config_capability = &chip_profile.sample_config_capability;
    let preferred = ImuSampleConfig {
        accel_range: RangeG(8),
        gyro_range: RangeDps(500),
        sample_rate_hz: SampleRateHz(100),
    };

    sample_config_capability
        .contains(&preferred)
        .then_some(preferred)
        .or_else(|| sample_config_capability.first_config())
        .unwrap_or(preferred)
}
