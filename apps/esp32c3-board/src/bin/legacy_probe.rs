#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

use embassy_executor::Spawner;
use embassy_time::Timer;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::spi::Mode;
use esp_hal::spi::master::{Config, Spi};
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use esp_println::println;
use smartimu::{
    BusId, SpiMode, SpiProfile, ImuBus, ImuChip, ImuDriver, ImuSampleConfig, ImuTargetId, RangeDps,
    RangeG, SampleRateHz,
};
use smartimu::drivers::{CandidateDriver, bmi270, hxy42688, icm42688, lsm6, qmi8658};
use smartimu::firmware::runtime::probe_first_matching;
use smartimu::EspImuBus;
use smartimu::EspDriverResources;

const SPI_FREQ_KHZ: u32 = 1_000;
const STREAM_INTERVAL_MS: u64 = 100;
const POWER_UP_DELAY_MS: u64 = 500;
const BUS_ID: BusId = BusId(0);

const PROFILE_MODE0: SpiProfile = SpiProfile::new(0, SpiMode::Mode0, SPI_FREQ_KHZ);
const PROFILE_MODE1: SpiProfile = SpiProfile::new(1, SpiMode::Mode1, SPI_FREQ_KHZ);
const PROFILE_MODE2: SpiProfile = SpiProfile::new(2, SpiMode::Mode2, SPI_FREQ_KHZ);
const PROFILE_MODE3: SpiProfile = SpiProfile::new(3, SpiMode::Mode3, SPI_FREQ_KHZ);
const PROFILE_MODE0_500K: SpiProfile = SpiProfile::new(4, SpiMode::Mode0, 500);
const PROFILE_MODE3_500K: SpiProfile = SpiProfile::new(5, SpiMode::Mode3, 500);
const PROFILE_MODE0_100K: SpiProfile = SpiProfile::new(6, SpiMode::Mode0, 100);
const PROFILE_MODE3_100K: SpiProfile = SpiProfile::new(7, SpiMode::Mode3, 100);

const PROFILES_MODE0: [SpiProfile; 1] = [PROFILE_MODE0];
const PROFILES_MODE3: [SpiProfile; 1] = [PROFILE_MODE3];
const PROFILES_MODE0_3: [SpiProfile; 2] = [PROFILE_MODE0, PROFILE_MODE3];
const PROFILES_ALL: [SpiProfile; 4] = [PROFILE_MODE0, PROFILE_MODE1, PROFILE_MODE2, PROFILE_MODE3];
const PROFILES_BMI: [SpiProfile; 8] = [
    PROFILE_MODE3,
    PROFILE_MODE0,
    PROFILE_MODE1,
    PROFILE_MODE2,
    PROFILE_MODE3_500K,
    PROFILE_MODE0_500K,
    PROFILE_MODE3_100K,
    PROFILE_MODE0_100K,
];

#[derive(Clone, Copy)]
struct ProbeConfig {
    label: &'static str,
    expected: ImuChip,
    target: ImuTargetId,
    candidates: &'static [CandidateDriver],
}

#[derive(Clone, Copy)]
struct Detected<'a> {
    name: &'static str,
    driver: &'a dyn ImuDriver,
    profile: SpiProfile,
    sample_config: ImuSampleConfig,
}

struct Runtime<'a> {
    config: &'static ProbeConfig,
    detected: Option<Detected<'a>>,
    sample_index: u32,
}

impl<'a> Runtime<'a> {
    const fn new(config: &'static ProbeConfig) -> Self {
        Self {
            config,
            detected: None,
            sample_index: 0,
        }
    }
}

static SLOT1_CANDIDATES: [CandidateDriver; 4] = [
    CandidateDriver {
        descriptor: &hxy42688::DESCRIPTOR,
        profiles: &PROFILES_MODE0_3,
    },
    CandidateDriver {
        descriptor: &lsm6::DESCRIPTOR,
        profiles: &PROFILES_MODE3,
    },
    CandidateDriver {
        descriptor: &icm42688::DESCRIPTOR,
        profiles: &PROFILES_MODE0,
    },
    CandidateDriver {
        descriptor: &qmi8658::DESCRIPTOR,
        profiles: &PROFILES_MODE0,
    },
];

static SLOT2_CANDIDATES: [CandidateDriver; 3] = [
    CandidateDriver {
        descriptor: &icm42688::DESCRIPTOR,
        profiles: &PROFILES_ALL,
    },
    CandidateDriver {
        descriptor: &qmi8658::DESCRIPTOR,
        profiles: &PROFILES_ALL,
    },
    CandidateDriver {
        descriptor: &lsm6::DESCRIPTOR,
        profiles: &PROFILES_MODE3,
    },
];

static SLOT3_CANDIDATES: [CandidateDriver; 2] = [
    CandidateDriver {
        descriptor: &bmi270::DESCRIPTOR,
        profiles: &PROFILES_BMI,
    },
    CandidateDriver {
        descriptor: &bmi270::DESCRIPTOR,
        profiles: &PROFILES_MODE0,
    },
];

static SLOT4_CANDIDATES: [CandidateDriver; 2] = [
    CandidateDriver {
        descriptor: &qmi8658::DESCRIPTOR,
        profiles: &PROFILES_MODE0,
    },
    CandidateDriver {
        descriptor: &icm42688::DESCRIPTOR,
        profiles: &PROFILES_MODE0,
    },
];

static SLOT5_CANDIDATES: [CandidateDriver; 4] = [
    CandidateDriver {
        descriptor: &lsm6::DESCRIPTOR,
        profiles: &PROFILES_MODE0_3,
    },
    CandidateDriver {
        descriptor: &hxy42688::DESCRIPTOR,
        profiles: &PROFILES_MODE0_3,
    },
    CandidateDriver {
        descriptor: &qmi8658::DESCRIPTOR,
        profiles: &PROFILES_MODE0_3,
    },
    CandidateDriver {
        descriptor: &icm42688::DESCRIPTOR,
        profiles: &PROFILES_MODE0,
    },
];

static PROBE_CONFIGS: [ProbeConfig; 5] = [
    ProbeConfig {
        label: "slot-1",
        expected: ImuChip::Icm42688Hxy,
        target: ImuTargetId {
            bus_id: BUS_ID,
            target_index: 0,
        },
        candidates: &SLOT1_CANDIDATES,
    },
    ProbeConfig {
        label: "slot-2",
        expected: ImuChip::Icm42688Pc,
        target: ImuTargetId {
            bus_id: BUS_ID,
            target_index: 1,
        },
        candidates: &SLOT2_CANDIDATES,
    },
    ProbeConfig {
        label: "slot-3",
        expected: ImuChip::Bmi270,
        target: ImuTargetId {
            bus_id: BUS_ID,
            target_index: 2,
        },
        candidates: &SLOT3_CANDIDATES,
    },
    ProbeConfig {
        label: "slot-4",
        expected: ImuChip::Qmi8658A,
        target: ImuTargetId {
            bus_id: BUS_ID,
            target_index: 3,
        },
        candidates: &SLOT4_CANDIDATES,
    },
    ProbeConfig {
        label: "slot-5",
        expected: ImuChip::Sc7u22,
        target: ImuTargetId {
            bus_id: BUS_ID,
            target_index: 4,
        },
        candidates: &SLOT5_CANDIDATES,
    },
];

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("panic: {}", info);
    loop {
        unsafe { core::arch::asm!("wfi") }
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

    println!("========================================");
    println!(" legacy_probe: old pin mapping IMU test ");
    println!("========================================");
    println!("SPI: SCK=GPIO6 MOSI=GPIO7 MISO=GPIO2");
    println!("CS order: GPIO8, GPIO4, GPIO3, GPIO5, GPIO1");

    let spi_config = Config::default()
        .with_frequency(Rate::from_khz(SPI_FREQ_KHZ))
        .with_mode(Mode::_0);
    let mut spi = Spi::new(peripherals.SPI2, spi_config)
        .unwrap()
        .with_sck(peripherals.GPIO6)
        .with_mosi(peripherals.GPIO7)
        .with_miso(peripherals.GPIO2);

    let targets = PROBE_CONFIGS.map(|config| config.target);
    let chip_selects = [
        Output::new(peripherals.GPIO8, Level::High, OutputConfig::default()),
        Output::new(peripherals.GPIO4, Level::High, OutputConfig::default()),
        Output::new(peripherals.GPIO3, Level::High, OutputConfig::default()),
        Output::new(peripherals.GPIO5, Level::High, OutputConfig::default()),
        Output::new(peripherals.GPIO1, Level::High, OutputConfig::default()),
    ];
    let mut bus = EspImuBus::new(&mut spi, targets, chip_selects);
    let resources = EspDriverResources;
    let mut runtimes = PROBE_CONFIGS.each_ref().map(|config| Runtime::new(config));

    Timer::after_millis(POWER_UP_DELAY_MS).await;

    for runtime in &mut runtimes {
        print_probe_snapshot(&mut bus, runtime.config.target, runtime.config.label);

        match probe_first_matching(&mut bus, runtime.config.target, runtime.config.candidates) {
            Ok(Some((driver, profile))) => {
                let sample_config = select_imu_sample_config(driver);
                match driver.reset(&mut bus, runtime.config.target).and_then(|_| {
                    driver.configure(&mut bus, runtime.config.target, &sample_config, &resources)
                }) {
                    Ok(()) => {
                        let name = runtime
                            .config
                            .candidates
                            .iter()
                            .find(|candidate| core::ptr::eq(candidate.descriptor.driver, driver))
                            .map(|candidate| candidate.descriptor.name)
                            .unwrap_or("unknown");
                        runtime.detected = Some(Detected {
                            name,
                            driver,
                            profile,
                            sample_config,
                        });
                        println!(
                            "{} expected={:?} detected={} actual={:?} profile={}khz/{:?}",
                            runtime.config.label,
                            runtime.config.expected,
                            name,
                            driver.chip(),
                            profile.frequency_khz,
                            profile.mode
                        );
                        if driver.chip() != runtime.config.expected {
                            println!(
                                "  !! mismatch: expected {:?}, got {:?}",
                                runtime.config.expected,
                                driver.chip()
                            );
                        }
                    }
                    Err(error) => {
                        println!(
                            "{} init failed for {:?}: {:?}",
                            runtime.config.label,
                            driver.chip(),
                            error
                        );
                    }
                }
            }
            Ok(None) => {
                println!(
                    "{} expected={:?} detected=unavailable",
                    runtime.config.label, runtime.config.expected
                );
            }
            Err(error) => {
                println!("{} probe error: {:?}", runtime.config.label, error);
            }
        }
    }

    println!("----------------------------------------");
    println!("Streaming detected IMUs with old mapping");
    println!("----------------------------------------");

    loop {
        for runtime in &mut runtimes {
            let Some(detected) = runtime.detected else {
                continue;
            };

            if let Err(error) = bus.apply_profile(runtime.config.target, detected.profile) {
                println!(
                    "{} profile switch failed: {:?}",
                    runtime.config.label, error
                );
                continue;
            }

            match detected.driver.read_raw(&mut bus, runtime.config.target) {
                Ok(raw) => {
                    runtime.sample_index = runtime.sample_index.wrapping_add(1);
                    let scale = smartimu::scale_profile_for_config(
                        detected.driver.chip(),
                        &detected.sample_config,
                    )
                    .unwrap_or_else(|| detected.driver.scale_profile());
                    let physical = raw.to_physical(scale);
                    println!(
                        "{} {} #{} raw[a=({},{},{}) g=({},{},{})] phys[a=({:.3},{:.3},{:.3}) g=({:.2},{:.2},{:.2})]",
                        runtime.config.label,
                        detected.name,
                        runtime.sample_index,
                        raw.accel[0],
                        raw.accel[1],
                        raw.accel[2],
                        raw.gyro[0],
                        raw.gyro[1],
                        raw.gyro[2],
                        physical.accel_g[0],
                        physical.accel_g[1],
                        physical.accel_g[2],
                        physical.gyro_dps[0],
                        physical.gyro_dps[1],
                        physical.gyro_dps[2],
                    );
                }
                Err(error) => {
                    println!("{} sample error: {:?}", runtime.config.label, error);
                }
            }
        }

        Timer::after_millis(STREAM_INTERVAL_MS).await;
    }
}

fn print_probe_snapshot(bus: &mut dyn ImuBus<Profile = SpiProfile>, target: ImuTargetId, label: &str) {
    let _ = bus.apply_profile(target, PROFILE_MODE0);
    let r00_m0 = bus.read_reg(target, 0x00, 0).ok();
    let r01_m0 = bus.read_reg(target, 0x01, 0).ok();
    let r05_m0 = bus.read_reg(target, 0x05, 0).ok();
    let r75_m0 = bus.read_reg(target, 0x75, 0).ok();
    let bmi00_d1_m0 = bus.read_reg(target, 0x00, 1).ok();

    let _ = bus.apply_profile(target, PROFILE_MODE3);
    let r00_m3 = bus.read_reg(target, 0x00, 0).ok();
    let r01_m3 = bus.read_reg(target, 0x01, 0).ok();
    let r05_m3 = bus.read_reg(target, 0x05, 0).ok();
    let r75_m3 = bus.read_reg(target, 0x75, 0).ok();
    let bmi00_d1_m3 = bus.read_reg(target, 0x00, 1).ok();

    println!(
        "{} probe m0[r00={:02X?} r01={:02X?} r05={:02X?} r75={:02X?} bmi_d1={:02X?}] m3[r00={:02X?} r01={:02X?} r05={:02X?} r75={:02X?} bmi_d1={:02X?}]",
        label,
        r00_m0,
        r01_m0,
        r05_m0,
        r75_m0,
        bmi00_d1_m0,
        r00_m3,
        r01_m3,
        r05_m3,
        r75_m3,
        bmi00_d1_m3,
    );
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
