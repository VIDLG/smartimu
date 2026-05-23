use alloc::borrow::Cow;
use alloc::string::String;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ImuId {
    /// Identifies the device or board that owns this IMU.
    pub system_id: u16,
    /// Identifies one IMU within the owning system.
    pub sensor_id: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
/// Identifies one physical or logical bus within a system.
pub struct BusId(pub u8);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemInfo {
    pub system_id: u16,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BusInfo {
    pub bus_id: BusId,
    pub label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImuChip {
    Icm42688Hxy,
    Icm42688Pc,
    Qmi8658A,
    Sc7u22,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RangeG(pub u16);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RangeDps(pub u16);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SampleRateHz(pub u16);

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TemperatureScale {
    pub c_per_lsb: f32,
    pub offset_c: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImuSampleConfig {
    pub accel_range: RangeG,
    pub gyro_range: RangeDps,
    pub sample_rate_hz: SampleRateHz,
}

/// Six-axis sampling support can be independent options or explicitly
/// constrained valid tuples.
///
/// Current implemented support:
/// - ICM-42688-HXY: independent accel 4/8/16 g, gyro 250/500/1000/2000 dps, 100 Hz.
/// - SC7I22/SC7U22: independent accel 4/8/16 g, gyro 250/500/1000 dps, 100 Hz.
/// - ICM-42688-PC: independent accel 2 g, gyro 2048 dps, 100 Hz.
/// - QMI8658A: independent accel 2 g, gyro 2048 dps, 100 Hz.
///
/// Temperature is intentionally modeled separately because most IMU
/// temperature channels do not share the accel/gyro range and ODR choices.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SampleConfigOptions {
    Independent {
        accel_ranges: Cow<'static, [RangeG]>,
        gyro_ranges: Cow<'static, [RangeDps]>,
        sample_rates: Cow<'static, [SampleRateHz]>,
    },
    Constrained {
        configs: Cow<'static, [ImuSampleConfig]>,
    },
}

impl SampleConfigOptions {
    pub fn contains(&self, config: &ImuSampleConfig) -> bool {
        match self {
            Self::Independent {
                accel_ranges,
                gyro_ranges,
                sample_rates,
            } => {
                accel_ranges.contains(&config.accel_range)
                    && gyro_ranges.contains(&config.gyro_range)
                    && sample_rates.contains(&config.sample_rate_hz)
            }
            Self::Constrained { configs } => configs.contains(config),
        }
    }

    pub fn first_config(&self) -> Option<ImuSampleConfig> {
        match self {
            Self::Independent {
                accel_ranges,
                gyro_ranges,
                sample_rates,
            } => Some(ImuSampleConfig {
                accel_range: *accel_ranges.first()?,
                gyro_range: *gyro_ranges.first()?,
                sample_rate_hz: *sample_rates.first()?,
            }),
            Self::Constrained { configs } => configs.first().copied(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TemperatureConfig {
    pub enabled: bool,
    pub scale: TemperatureScale,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SampleReadoutSupport {
    pub temperature: bool,
    pub sensor_timestamp: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImuChipProfile {
    pub chip: ImuChip,
    pub sample_config_options: SampleConfigOptions,
    pub sample_readout_support: SampleReadoutSupport,
    pub temperature_config: Option<TemperatureConfig>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImuIdentity {
    pub who_am_i: u8,
    pub revision: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProbeInfo {
    pub chip_profile: ImuChipProfile,
    pub identity: ImuIdentity,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImuInfo {
    pub id: ImuId,
    pub bus_id: BusId,
    pub chip_profile: ImuChipProfile,
    pub label: Option<String>,
    pub sample_config: ImuSampleConfig,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Quaternion {
    pub w: f32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}
