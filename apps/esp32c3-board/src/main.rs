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
use smartimu::{
    BusDescriptor, ImuBus, ImuChip, ImuDescriptor, ImuDriver, ImuSampleConfig, OrientationFrame,
    RangeDps, RangeG, SampleRateHz, SpiProfile, WireFormat, encode_binary_packet, encode_json,
};
use smartimu::firmware::runtime::probe_first_matching;
use smartimu::firmware::transport::{SessionRuntime, bounded_string};
use smartimu::fusion::{FusionFilter, FusionFilterSettings};
use smartimu::EspImuBus;
use smartimu::EspDriverResources;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    esp_println::println!("panic: {}", info);
    loop {
        unsafe { core::arch::asm!("wfi") }
    }
}

#[derive(Clone, Copy)]
struct DetectedImu {
    name: &'static str,
    driver: &'static dyn ImuDriver,
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

    fn emit_frame(&mut self, frame: &smartimu::WireFrame) {
        match self.mode {
            board::TransportMode::Json => match encode_json::<768>(frame) {
                Ok(line) => {
                    println!("{}", line);
                }
                Err(_) => {}
            },
            board::TransportMode::Binary => match encode_binary_packet::<1024>(frame) {
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

    let targets = board::BOARD_IMUS.map(|config| config.target);
    let chip_selects = [
        Output::new(peripherals.GPIO8, Level::High, OutputConfig::default()),
        Output::new(peripherals.GPIO4, Level::High, OutputConfig::default()),
        Output::new(peripherals.GPIO3, Level::High, OutputConfig::default()),
        Output::new(peripherals.GPIO5, Level::High, OutputConfig::default()),
        Output::new(peripherals.GPIO1, Level::High, OutputConfig::default()),
    ];
    let mut bus = EspImuBus::new(&mut spi, targets, chip_selects);
    let usb = match board::TRANSPORT_MODE {
        board::TransportMode::Json => None,
        board::TransportMode::Binary => Some(UsbSerialJtag::new(peripherals.USB_DEVICE)),
    };
    let mut transport = Transport::new(usb, board::TRANSPORT_MODE);
    let resources = EspDriverResources;
    let boot = Instant::now();
    let mut session = SessionRuntime::new(board::SYSTEM_ID, 1, transport.format());

    let mut runtimes = board::BOARD_IMUS
        .each_ref()
        .map(|config| ImuRuntime::new(config));
    let mut heartbeat_count: u32 = 0;

    Timer::after_millis(board::POWER_UP_DELAY_MS).await;

    transport.emit_frame(&session.hello(uptime_ms(boot), "esp32c3-board"));

    for runtime in &mut runtimes {
        match probe_first_matching(&mut bus, runtime.config.target, runtime.config.candidates) {
            Ok(Some((driver, profile))) => {
                let sample_config = select_imu_sample_config(driver);
                let result = driver.reset(&mut bus, runtime.config.target).and_then(|_| {
                    driver.configure(&mut bus, runtime.config.target, &sample_config, &resources)
                });

                match result {
                    Ok(()) => {
                        runtime.detected = Some(DetectedImu {
                            name: runtime
                                .config
                                .candidates
                                .iter()
                                .find(|candidate| {
                                    core::ptr::eq(candidate.descriptor.driver, driver)
                                })
                                .map(|candidate| candidate.descriptor.name)
                                .unwrap_or("unknown"),
                            driver,
                            profile,
                            sample_config,
                        });
                        let mut fusion_settings = FusionFilterSettings::default();
                        fusion_settings.gyroscope_range_dps = sample_config.gyro_range.0 as f32;
                        runtime.fusion = Some(FusionFilter::new(fusion_settings));
                        runtime.last_orientation_timestamp_us = None;
                        transport.emit_frame(&session.probe_result(
                            uptime_ms(boot),
                            runtime.config.imu_id,
                            runtime.detected.unwrap().name,
                            driver.chip(),
                            true,
                            None,
                            Some(profile),
                        ));
                        if driver.chip() != runtime.config.expected {
                            transport.emit_frame(&session.error(
                                uptime_ms(boot),
                                Some(runtime.config.imu_id),
                                smartimu::ImuError::ChipNotFound,
                                "detected chip mismatches expected chip",
                            ));
                        }
                    }
                    Err(error) => {
                        transport.emit_frame(&session.probe_result(
                            uptime_ms(boot),
                            runtime.config.imu_id,
                            "probe-match",
                            driver.chip(),
                            false,
                            Some(error),
                            None,
                        ));
                    }
                }
            }
            Ok(None) => {
                transport.emit_frame(&session.probe_result(
                    uptime_ms(boot),
                    runtime.config.imu_id,
                    "none",
                    ImuChip::Unknown,
                    false,
                    None,
                    None,
                ));
                transport.emit_frame(&session.error(
                    uptime_ms(boot),
                    Some(runtime.config.imu_id),
                    smartimu::ImuError::ChipNotFound,
                    &probe_snapshot(&mut bus, runtime.config.target, runtime.config.expected),
                ));
            }
            Err(error) => {
                transport.emit_frame(&session.error(
                    uptime_ms(boot),
                    Some(runtime.config.imu_id),
                    error,
                    "probe error",
                ));
            }
        }
    }

    transport.emit_frame(&session.topology(
        uptime_ms(boot),
        bus_descriptors(),
        imu_descriptors(&runtimes),
    ));

    loop {
        let mut active_imus = 0u16;
        for runtime in &mut runtimes {
            let Some(detected) = runtime.detected else {
                continue;
            };
            active_imus += 1;

            if let Err(error) = bus.apply_profile(runtime.config.target, detected.profile) {
                transport.emit_frame(&session.error(
                    uptime_ms(boot),
                    Some(runtime.config.imu_id),
                    error,
                    "profile switch failed",
                ));
                continue;
            }

            match detected.driver.read_raw(&mut bus, runtime.config.target) {
                Ok(raw) => {
                    runtime.sample_index = runtime.sample_index.wrapping_add(1);
                    let timestamp_us = Instant::now().duration_since(boot).as_micros();
                    transport.emit_frame(&session.sample(
                        uptime_ms(boot),
                        runtime.config.imu_id,
                        detected.driver.chip(),
                        runtime.sample_index,
                        timestamp_us,
                        raw,
                        0x0001,
                    ));

                    if let Some(scale) = smartimu::scale_profile_for_config(
                        detected.driver.chip(),
                        &detected.sample_config,
                    ) {
                        if let Some(fusion) = runtime.fusion.as_mut() {
                            let physical = raw.to_physical(scale);
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
                                ((timestamp_us.saturating_sub(last)) as f32 / 1_000_000.0)
                                    .clamp(0.0, 0.1)
                            } else {
                                0.0
                            };
                            runtime.last_orientation_timestamp_us = Some(timestamp_us);
                            let quaternion = fusion.update_imu(accel_ms2, gyro_rads, dt_s);
                            transport.emit_frame(&smartimu::WireFrame::Orientation(
                                OrientationFrame {
                                    header: session.header(uptime_ms(boot)),
                                    imu_id: runtime.config.imu_id,
                                    imu_chip: detected.driver.chip(),
                                    sample_index: runtime.sample_index,
                                    timestamp_us,
                                    quaternion,
                                },
                            ));
                        }
                    }
                }
                Err(error) => {
                    transport.emit_frame(&session.error(
                        uptime_ms(boot),
                        Some(runtime.config.imu_id),
                        error,
                        "sample error",
                    ));
                }
            }
        }

        transport.emit_frame(&session.heartbeat(uptime_ms(boot), active_imus));
        heartbeat_count = heartbeat_count.wrapping_add(1);
        if heartbeat_count % 20 == 0 {
            transport.emit_frame(&session.hello(uptime_ms(boot), "esp32c3-board"));
            transport.emit_frame(&session.topology(
                uptime_ms(boot),
                bus_descriptors(),
                imu_descriptors(&runtimes),
            ));
        }

        Timer::after_millis(board::STREAM_INTERVAL_MS).await;
    }
}

fn uptime_ms(boot: Instant) -> u32 {
    Instant::now().duration_since(boot).as_millis() as u32
}

fn bus_descriptors() -> Vec<BusDescriptor> {
    let mut buses = Vec::new();
    buses.push(BusDescriptor {
        bus_id: board::BUS_ID,
        label: bounded_string("spi2", smartimu::MAX_LABEL_LEN),
    });
    buses
}

fn imu_descriptors(runtimes: &[ImuRuntime; 5]) -> Vec<ImuDescriptor> {
    let mut imus = Vec::new();
    for runtime in runtimes {
        let chip = runtime
            .detected
            .map(|detected| detected.driver.chip())
            .unwrap_or(ImuChip::Unknown);
        let supported_sample_configs = runtime
            .detected
            .map(|detected| detected.driver.supported_sample_configs())
            .unwrap_or_default();
        let sample_config = runtime.detected.map(|detected| detected.sample_config);

        let descriptor = ImuDescriptor {
            id: runtime.config.imu_id,
            bus_id: board::BUS_ID,
            chip,
            label: bounded_string(runtime.config.label, smartimu::MAX_LABEL_LEN),
            sample_config,
            supported_sample_configs,
        };
        imus.push(descriptor);
    }
    imus
}

fn select_imu_sample_config(driver: &dyn ImuDriver) -> ImuSampleConfig {
    let supported_sample_configs = driver.supported_sample_configs();
    let preferred = ImuSampleConfig {
        accel_range: RangeG(8),
        gyro_range: RangeDps(500),
        sample_rate_hz: SampleRateHz(100),
    };

    supported_sample_configs
        .iter()
        .copied()
        .find(|config| *config == preferred)
        .or_else(|| supported_sample_configs.first().copied())
        .unwrap_or(preferred)
}

fn probe_snapshot(
    bus: &mut dyn ImuBus<Profile = SpiProfile>,
    target: smartimu::ImuTargetId,
    expected: ImuChip,
) -> String {
    if expected == ImuChip::Bmi270 {
        return bmi270_probe_snapshot(bus, target);
    }

    let _ = bus.apply_profile(target, board::PROFILE_MODE0);
    let r00_m0 = bus.read_reg(target, 0x00, 0).ok().unwrap_or(0xFF);
    let r01_m0 = bus.read_reg(target, 0x01, 0).ok().unwrap_or(0xFF);
    let r75_m0 = bus.read_reg(target, 0x75, 0).ok().unwrap_or(0xFF);

    let _ = bus.apply_profile(target, board::PROFILE_MODE3);
    let r00_m3 = bus.read_reg(target, 0x00, 0).ok().unwrap_or(0xFF);
    let r01_m3 = bus.read_reg(target, 0x01, 0).ok().unwrap_or(0xFF);
    let r75_m3 = bus.read_reg(target, 0x75, 0).ok().unwrap_or(0xFF);

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

fn bmi270_probe_snapshot(bus: &mut dyn ImuBus<Profile = SpiProfile>, target: smartimu::ImuTargetId) -> String {
    let _ = bus.apply_profile(target, board::PROFILE_MODE0_100K);
    let r00_m0_d0 = bus.read_reg(target, 0x00, 0).ok().unwrap_or(0xFF);
    let r00_m0_d1 = bus.read_reg(target, 0x00, 1).ok().unwrap_or(0xFF);
    let r21_m0_d1 = bus.read_reg(target, 0x21, 1).ok().unwrap_or(0xFF);

    let _ = bus.apply_profile(target, board::PROFILE_MODE3_100K);
    let r00_m3_d0 = bus.read_reg(target, 0x00, 0).ok().unwrap_or(0xFF);
    let r00_m3_d1 = bus.read_reg(target, 0x00, 1).ok().unwrap_or(0xFF);
    let r21_m3_d1 = bus.read_reg(target, 0x21, 1).ok().unwrap_or(0xFF);

    let mut out = String::new();
    let _ = core::fmt::write(
        &mut out,
        format_args!(
            "bmi m0 d0={:02X} d1={:02X} s={:02X} m3 d0={:02X} d1={:02X} s={:02X}",
            r00_m0_d0, r00_m0_d1, r21_m0_d1, r00_m3_d0, r00_m3_d1, r21_m3_d1
        ),
    );
    out
}
