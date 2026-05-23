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
use imu_core::{BusId, SpiMode, SpiProfile, ImuBus, ImuTargetId};
use imu_platform_esp::bus::EspImuBus;

const BUS_ID: BusId = BusId(0);
const TARGET: ImuTargetId = ImuTargetId {
    bus_id: BUS_ID,
    target_index: 0,
};

const MODE0_100K: SpiProfile = SpiProfile::new(0, SpiMode::Mode0, 100);
const MODE0_500K: SpiProfile = SpiProfile::new(1, SpiMode::Mode0, 500);
const MODE0_1M: SpiProfile = SpiProfile::new(2, SpiMode::Mode0, 1_000);
const MODE3_100K: SpiProfile = SpiProfile::new(3, SpiMode::Mode3, 100);
const MODE3_500K: SpiProfile = SpiProfile::new(4, SpiMode::Mode3, 500);
const MODE3_1M: SpiProfile = SpiProfile::new(5, SpiMode::Mode3, 1_000);

const PROFILES: [SpiProfile; 6] = [
    MODE0_100K, MODE0_500K, MODE0_1M, MODE3_100K, MODE3_500K, MODE3_1M,
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
    println!(" BMI270 dedicated probe on GPIO3 / slot3 ");
    println!("========================================");
    println!("SPI: SCK=GPIO6 MOSI=GPIO7 MISO=GPIO2");
    println!("CS : GPIO3 only");

    let spi_config = Config::default()
        .with_frequency(Rate::from_khz(100))
        .with_mode(Mode::_0);
    let mut spi = Spi::new(peripherals.SPI2, spi_config)
        .unwrap()
        .with_sck(peripherals.GPIO6)
        .with_mosi(peripherals.GPIO7)
        .with_miso(peripherals.GPIO2);

    let targets = [TARGET];
    let chip_selects = [Output::new(
        peripherals.GPIO3,
        Level::High,
        OutputConfig::default(),
    )];
    let mut bus = EspImuBus::new(&mut spi, targets, chip_selects);

    Timer::after_millis(500).await;

    dump_profiles(&mut bus);

    println!("-- issuing soft reset 0x7E=0xB6 --");
    let reset_result = bus.write_reg(TARGET, 0x7E, 0xB6);
    println!("reset write result: {:?}", reset_result);
    Timer::after_millis(50).await;

    dump_profiles(&mut bus);

    println!("-- continuous poll --");
    loop {
        let _ = bus.apply_profile(TARGET, MODE0_100K);
        let chip_id = bus.read_reg(TARGET, 0x00, 1).ok();
        let status = bus.read_reg(TARGET, 0x21, 1).ok();
        let pwr = bus.read_reg(TARGET, 0x7C, 1).ok();
        println!(
            "poll m0@100k chip_id={:02X?} status={:02X?} pwr={:02X?}",
            chip_id, status, pwr
        );
        Timer::after_millis(1000).await;
    }
}

fn dump_profiles(bus: &mut dyn ImuBus<Profile = SpiProfile>) {
    for profile in PROFILES {
        let _ = bus.apply_profile(TARGET, profile);
        bus.delay_ms(2);

        let r00_d0 = bus.read_reg(TARGET, 0x00, 0).ok();
        let r00_d1 = bus.read_reg(TARGET, 0x00, 1).ok();
        let r00_d2 = bus.read_reg(TARGET, 0x00, 2).ok();
        let r01_d0 = bus.read_reg(TARGET, 0x01, 0).ok();
        let r21_d1 = bus.read_reg(TARGET, 0x21, 1).ok();
        let r7c_d1 = bus.read_reg(TARGET, 0x7C, 1).ok();

        println!(
            "profile id={} mode={:?} freq={}k r00[d0={:02X?} d1={:02X?} d2={:02X?}] r01={:02X?} r21d1={:02X?} r7Cd1={:02X?}",
            profile.id,
            profile.mode,
            profile.frequency_khz,
            r00_d0,
            r00_d1,
            r00_d2,
            r01_d0,
            r21_d1,
            r7c_d1,
        );
    }
}
