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
use smartimu::EspImuBus;
use smartimu::drivers::{hxy42688, icm42688, lsm6, qmi8658};
use smartimu::{
    BusId, CandidateDriver, ImuBus, ImuChip, ImuChipProfile, ImuDriver, ImuSampleConfig,
    ImuTargetId, ProbePlan, RangeDps, RangeG, SampleRateHz, SpiMode, SpiProfile, Turnaround, probe,
};

const SPI_FREQ_KHZ: u32 = 1_000;
const STREAM_INTERVAL_MS: u64 = 100;
const POWER_UP_DELAY_MS: u64 = 500;
const BUS_ID: BusId = BusId(0);

const PROFILE_MODE0: SpiProfile = SpiProfile::new(0, SpiMode::Mode0, SPI_FREQ_KHZ);
const PROFILE_MODE1: SpiProfile = SpiProfile::new(1, SpiMode::Mode1, SPI_FREQ_KHZ);
const PROFILE_MODE2: SpiProfile = SpiProfile::new(2, SpiMode::Mode2, SPI_FREQ_KHZ);
const PROFILE_MODE3: SpiProfile = SpiProfile::new(3, SpiMode::Mode3, SPI_FREQ_KHZ);

const PROFILES_MODE0: [SpiProfile; 1] = [PROFILE_MODE0];
const PROFILES_MODE3: [SpiProfile; 1] = [PROFILE_MODE3];
const PROFILES_MODE0_3: [SpiProfile; 2] = [PROFILE_MODE0, PROFILE_MODE3];
const PROFILES_ALL: [SpiProfile; 4] = [PROFILE_MODE0, PROFILE_MODE1, PROFILE_MODE2, PROFILE_MODE3];

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
        info: &hxy42688::INFO,
        profiles: &PROFILES_MODE0_3,
    },
    CandidateDriver {
        info: &lsm6::INFO,
        profiles: &PROFILES_MODE3,
    },
    CandidateDriver {
        info: &icm42688::INFO,
        profiles: &PROFILES_MODE0,
    },
    CandidateDriver {
        info: &qmi8658::INFO,
        profiles: &PROFILES_MODE0,
    },
];

static SLOT2_CANDIDATES: [CandidateDriver; 3] = [
    CandidateDriver {
        info: &icm42688::INFO,
        profiles: &PROFILES_ALL,
    },
    CandidateDriver {
        info: &qmi8658::INFO,
        profiles: &PROFILES_ALL,
    },
    CandidateDriver {
        info: &lsm6::INFO,
        profiles: &PROFILES_MODE3,
    },
];

static SLOT3_CANDIDATES: [CandidateDriver; 0] = [];

static SLOT4_CANDIDATES: [CandidateDriver; 2] = [
    CandidateDriver {
        info: &qmi8658::INFO,
        profiles: &PROFILES_MODE0,
    },
    CandidateDriver {
        info: &icm42688::INFO,
        profiles: &PROFILES_MODE0,
    },
];

static SLOT5_CANDIDATES: [CandidateDriver; 4] = [
    CandidateDriver {
        info: &lsm6::INFO,
        profiles: &PROFILES_MODE0_3,
    },
    CandidateDriver {
        info: &hxy42688::INFO,
        profiles: &PROFILES_MODE0_3,
    },
    CandidateDriver {
        info: &qmi8658::INFO,
        profiles: &PROFILES_MODE0_3,
    },
    CandidateDriver {
        info: &icm42688::INFO,
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
        expected: ImuChip::Icm42688Pc,
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
    init_heap();

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt =
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

    println!("========================================");
    println!(" legacy_probe: old pin mapping IMU test ");
    println!("========================================");
    println!("SPI: SCK=GPIO6 MOSI=GPIO7 MISO=GPIO2");
    println!("CS order: GPIO8, GPIO4, GPIO5, GPIO1");

    let spi_config = Config::default()
        .with_frequency(Rate::from_khz(SPI_FREQ_KHZ))
        .with_mode(Mode::_0);
    let mut spi = Spi::new(peripherals.SPI2, spi_config)
        .unwrap()
        .with_sck(peripherals.GPIO6)
        .with_mosi(peripherals.GPIO7)
        .with_miso(peripherals.GPIO2);

    let mut bus = EspImuBus::new(&mut spi)
        .with_target(
            PROBE_CONFIGS[0].target,
            Output::new(peripherals.GPIO8, Level::High, OutputConfig::default()),
        )
        .with_target(
            PROBE_CONFIGS[1].target,
            Output::new(peripherals.GPIO4, Level::High, OutputConfig::default()),
        )
        .with_target(
            PROBE_CONFIGS[3].target,
            Output::new(peripherals.GPIO5, Level::High, OutputConfig::default()),
        )
        .with_target(
            PROBE_CONFIGS[4].target,
            Output::new(peripherals.GPIO1, Level::High, OutputConfig::default()),
        );
    let mut runtimes = PROBE_CONFIGS.each_ref().map(|config| Runtime::new(config));

    Timer::after_millis(POWER_UP_DELAY_MS).await;

    for runtime in &mut runtimes {
        print_probe_snapshot(&mut bus, runtime.config.target, runtime.config.label);

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
                let chip_profile = probe_match.info.chip_profile;
                let identity = probe_match.info.identity;
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
                        let name = runtime
                            .config
                            .candidates
                            .iter()
                            .find(|candidate| core::ptr::eq(candidate.info.driver, driver))
                            .map(|candidate| candidate.info.name)
                            .unwrap_or("unknown");
                        runtime.detected = Some(Detected {
                            name,
                            driver,
                            profile,
                            sample_config,
                        });
                        println!(
                            "{} expected={:?} detected={} actual={:?} id=0x{:02X} rev={:?} profile={}khz/{:?}",
                            runtime.config.label,
                            runtime.config.expected,
                            name,
                            chip_profile.chip,
                            identity.who_am_i,
                            identity.revision,
                            profile.frequency_khz,
                            profile.mode
                        );
                        if chip_profile.chip != runtime.config.expected {
                            println!(
                                "  !! mismatch: expected {:?}, got {:?}",
                                runtime.config.expected, chip_profile.chip
                            );
                        }
                    }
                    Err(error) => {
                        println!(
                            "{} init failed for {:?}: {:?}",
                            runtime.config.label, chip_profile.chip, error
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
                    let scale = smartimu::Imu6Scale::from(detected.sample_config);
                    let physical = raw.imu6.to_physical(scale);
                    println!(
                        "{} {} #{} raw[a=({},{},{}) g=({},{},{})] phys[a=({:.3},{:.3},{:.3}) g=({:.2},{:.2},{:.2})]",
                        runtime.config.label,
                        detected.name,
                        runtime.sample_index,
                        raw.imu6.accel[0],
                        raw.imu6.accel[1],
                        raw.imu6.accel[2],
                        raw.imu6.gyro[0],
                        raw.imu6.gyro[1],
                        raw.imu6.gyro[2],
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

fn init_heap() {
    esp_alloc::heap_allocator!(size: 32 * 1024);
}

fn print_probe_snapshot(bus: &mut dyn ImuBus, target: ImuTargetId, label: &str) {
    let _ = bus.apply_profile(target, PROFILE_MODE0);
    let r00_m0 = bus.read_reg(target, 0x00, Turnaround(0)).ok();
    let r01_m0 = bus.read_reg(target, 0x01, Turnaround(0)).ok();
    let r05_m0 = bus.read_reg(target, 0x05, Turnaround(0)).ok();
    let r75_m0 = bus.read_reg(target, 0x75, Turnaround(0)).ok();
    let bmi00_d1_m0 = bus.read_reg(target, 0x00, Turnaround(1)).ok();

    let _ = bus.apply_profile(target, PROFILE_MODE3);
    let r00_m3 = bus.read_reg(target, 0x00, Turnaround(0)).ok();
    let r01_m3 = bus.read_reg(target, 0x01, Turnaround(0)).ok();
    let r05_m3 = bus.read_reg(target, 0x05, Turnaround(0)).ok();
    let r75_m3 = bus.read_reg(target, 0x75, Turnaround(0)).ok();
    let bmi00_d1_m3 = bus.read_reg(target, 0x00, Turnaround(1)).ok();

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
