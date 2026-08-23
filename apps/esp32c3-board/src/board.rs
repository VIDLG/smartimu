use smartimu::drivers::{hxy42688, icm42688, lsm6, qmi8658};
use smartimu::{
    BusId, CandidateDriver, ImuChipModel, ImuId, ImuTargetId, SensorId, SpiMode, SpiProfile,
    SystemId,
};

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportMode {
    Json,
    Binary,
}

pub const SPI_FREQ_KHZ: u32 = 1_000;
pub const STREAM_INTERVAL_MS: u64 = 5;
pub const POWER_UP_DELAY_MS: u64 = 500;
pub const SYSTEM_ID: SystemId = SystemId(1);
pub const BUS_ID: BusId = BusId(0);
#[cfg(feature = "json-transport")]
pub const TRANSPORT_MODE: TransportMode = TransportMode::Json;
#[cfg(feature = "binary-transport")]
pub const TRANSPORT_MODE: TransportMode = TransportMode::Binary;

pub const PROFILE_MODE0: SpiProfile = SpiProfile::new(0, SpiMode::Mode0, SPI_FREQ_KHZ);
pub const PROFILE_MODE1: SpiProfile = SpiProfile::new(1, SpiMode::Mode1, SPI_FREQ_KHZ);
pub const PROFILE_MODE2: SpiProfile = SpiProfile::new(2, SpiMode::Mode2, SPI_FREQ_KHZ);
pub const PROFILE_MODE3: SpiProfile = SpiProfile::new(3, SpiMode::Mode3, SPI_FREQ_KHZ);

pub const PROFILES_MODE0: [SpiProfile; 1] = [PROFILE_MODE0];
pub const PROFILES_MODE3: [SpiProfile; 1] = [PROFILE_MODE3];
pub const PROFILES_MODE0_3: [SpiProfile; 2] = [PROFILE_MODE0, PROFILE_MODE3];
pub const PROFILES_ALL: [SpiProfile; 4] =
    [PROFILE_MODE0, PROFILE_MODE1, PROFILE_MODE2, PROFILE_MODE3];

#[derive(Clone, Copy)]
pub struct BoardImuConfig {
    pub imu_id: ImuId,
    pub target: ImuTargetId,
    pub label: &'static str,
    pub expected: ImuChipModel,
    pub candidates: &'static [CandidateDriver],
}

pub static SLOT1_CANDIDATES: [CandidateDriver; 4] = [
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

pub static SLOT2_CANDIDATES: [CandidateDriver; 3] = [
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

pub static SLOT3_CANDIDATES: [CandidateDriver; 0] = [];

pub static SLOT4_CANDIDATES: [CandidateDriver; 2] = [
    CandidateDriver {
        info: &qmi8658::INFO,
        profiles: &PROFILES_MODE0,
    },
    CandidateDriver {
        info: &icm42688::INFO,
        profiles: &PROFILES_MODE0,
    },
];

pub static SLOT5_CANDIDATES: [CandidateDriver; 4] = [
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

pub static BOARD_IMUS: [BoardImuConfig; 5] = [
    BoardImuConfig {
        imu_id: ImuId {
            system_id: SYSTEM_ID,
            sensor_id: SensorId(1),
        },
        target: ImuTargetId {
            bus_id: BUS_ID,
            target_index: 0,
        },
        label: "slot-1",
        expected: ImuChipModel::Icm42688Hxy,
        candidates: &SLOT1_CANDIDATES,
    },
    BoardImuConfig {
        imu_id: ImuId {
            system_id: SYSTEM_ID,
            sensor_id: SensorId(2),
        },
        target: ImuTargetId {
            bus_id: BUS_ID,
            target_index: 1,
        },
        label: "slot-2",
        expected: ImuChipModel::Icm42688Pc,
        candidates: &SLOT2_CANDIDATES,
    },
    BoardImuConfig {
        imu_id: ImuId {
            system_id: SYSTEM_ID,
            sensor_id: SensorId(3),
        },
        target: ImuTargetId {
            bus_id: BUS_ID,
            target_index: 2,
        },
        label: "slot-3",
        expected: ImuChipModel::Icm42688Pc,
        candidates: &SLOT3_CANDIDATES,
    },
    BoardImuConfig {
        imu_id: ImuId {
            system_id: SYSTEM_ID,
            sensor_id: SensorId(4),
        },
        target: ImuTargetId {
            bus_id: BUS_ID,
            target_index: 3,
        },
        label: "slot-4",
        expected: ImuChipModel::Qmi8658A,
        candidates: &SLOT4_CANDIDATES,
    },
    BoardImuConfig {
        imu_id: ImuId {
            system_id: SYSTEM_ID,
            sensor_id: SensorId(5),
        },
        target: ImuTargetId {
            bus_id: BUS_ID,
            target_index: 4,
        },
        label: "slot-5",
        expected: ImuChipModel::Sc7u22,
        candidates: &SLOT5_CANDIDATES,
    },
];
