use imu_core::{BusId, SpiMode, SpiProfile, ImuChip, ImuId, ImuTargetId};
use imu_drivers::{CandidateDriver, bmi270, hxy42688, icm42688, lsm6, qmi8658};

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportMode {
    Json,
    Binary,
}

pub const SPI_FREQ_KHZ: u32 = 1_000;
pub const STREAM_INTERVAL_MS: u64 = 5;
pub const POWER_UP_DELAY_MS: u64 = 500;
pub const SYSTEM_ID: u16 = 1;
pub const BUS_ID: BusId = BusId(0);
#[cfg(feature = "json-transport")]
pub const TRANSPORT_MODE: TransportMode = TransportMode::Json;
#[cfg(feature = "binary-transport")]
pub const TRANSPORT_MODE: TransportMode = TransportMode::Binary;

pub const PROFILE_MODE0: SpiProfile = SpiProfile::new(0, SpiMode::Mode0, SPI_FREQ_KHZ);
pub const PROFILE_MODE1: SpiProfile = SpiProfile::new(1, SpiMode::Mode1, SPI_FREQ_KHZ);
pub const PROFILE_MODE2: SpiProfile = SpiProfile::new(2, SpiMode::Mode2, SPI_FREQ_KHZ);
pub const PROFILE_MODE3: SpiProfile = SpiProfile::new(3, SpiMode::Mode3, SPI_FREQ_KHZ);
pub const PROFILE_MODE0_500K: SpiProfile = SpiProfile::new(4, SpiMode::Mode0, 500);
pub const PROFILE_MODE3_500K: SpiProfile = SpiProfile::new(5, SpiMode::Mode3, 500);
pub const PROFILE_MODE0_100K: SpiProfile = SpiProfile::new(6, SpiMode::Mode0, 100);
pub const PROFILE_MODE3_100K: SpiProfile = SpiProfile::new(7, SpiMode::Mode3, 100);
pub const SLOT3_OPTIONAL: bool = true;

pub const PROFILES_MODE0: [SpiProfile; 1] = [PROFILE_MODE0];
pub const PROFILES_MODE3: [SpiProfile; 1] = [PROFILE_MODE3];
pub const PROFILES_MODE0_3: [SpiProfile; 2] = [PROFILE_MODE0, PROFILE_MODE3];
pub const PROFILES_ALL: [SpiProfile; 4] =
    [PROFILE_MODE0, PROFILE_MODE1, PROFILE_MODE2, PROFILE_MODE3];
pub const PROFILES_BMI: [SpiProfile; 8] = [
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
pub struct BoardImuConfig {
    pub imu_id: ImuId,
    pub target: ImuTargetId,
    pub label: &'static str,
    pub expected: ImuChip,
    pub candidates: &'static [CandidateDriver],
}

pub static SLOT1_CANDIDATES: [CandidateDriver; 4] = [
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

pub static SLOT2_CANDIDATES: [CandidateDriver; 3] = [
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

pub static SLOT3_CANDIDATES: [CandidateDriver; 2] = [
    CandidateDriver {
        descriptor: &bmi270::DESCRIPTOR,
        profiles: &PROFILES_BMI,
    },
    CandidateDriver {
        descriptor: &bmi270::DESCRIPTOR,
        profiles: &PROFILES_MODE0,
    },
];

pub static SLOT4_CANDIDATES: [CandidateDriver; 2] = [
    CandidateDriver {
        descriptor: &qmi8658::DESCRIPTOR,
        profiles: &PROFILES_MODE0,
    },
    CandidateDriver {
        descriptor: &icm42688::DESCRIPTOR,
        profiles: &PROFILES_MODE0,
    },
];

pub static SLOT5_CANDIDATES: [CandidateDriver; 4] = [
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

pub static BOARD_IMUS: [BoardImuConfig; 5] = [
    BoardImuConfig {
        imu_id: ImuId {
            system_id: SYSTEM_ID,
            sensor_id: 1,
        },
        target: ImuTargetId {
            bus_id: BUS_ID,
            target_index: 0,
        },
        label: "slot-1",
        expected: ImuChip::Icm42688Hxy,
        candidates: &SLOT1_CANDIDATES,
    },
    BoardImuConfig {
        imu_id: ImuId {
            system_id: SYSTEM_ID,
            sensor_id: 2,
        },
        target: ImuTargetId {
            bus_id: BUS_ID,
            target_index: 1,
        },
        label: "slot-2",
        expected: ImuChip::Icm42688Pc,
        candidates: &SLOT2_CANDIDATES,
    },
    BoardImuConfig {
        imu_id: ImuId {
            system_id: SYSTEM_ID,
            sensor_id: 3,
        },
        target: ImuTargetId {
            bus_id: BUS_ID,
            target_index: 2,
        },
        label: "slot-3",
        expected: ImuChip::Bmi270,
        candidates: &SLOT3_CANDIDATES,
    },
    BoardImuConfig {
        imu_id: ImuId {
            system_id: SYSTEM_ID,
            sensor_id: 4,
        },
        target: ImuTargetId {
            bus_id: BUS_ID,
            target_index: 3,
        },
        label: "slot-4",
        expected: ImuChip::Qmi8658A,
        candidates: &SLOT4_CANDIDATES,
    },
    BoardImuConfig {
        imu_id: ImuId {
            system_id: SYSTEM_ID,
            sensor_id: 5,
        },
        target: ImuTargetId {
            bus_id: BUS_ID,
            target_index: 4,
        },
        label: "slot-5",
        expected: ImuChip::Sc7u22,
        candidates: &SLOT5_CANDIDATES,
    },
];
