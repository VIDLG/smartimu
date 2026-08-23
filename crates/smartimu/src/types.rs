use alloc::borrow::Cow;
use alloc::string::String;
use derive_more::{Display, From, Into};
use serde::{Deserialize, Serialize};

#[derive(
    Clone, Copy, Debug, Default, Display, From, Into, PartialEq, Eq, Hash, Serialize, Deserialize,
)]
pub struct SystemId(pub u16);

#[derive(
    Clone, Copy, Debug, Default, Display, From, Into, PartialEq, Eq, Hash, Serialize, Deserialize,
)]
pub struct SessionId(pub u32);

#[derive(
    Clone, Copy, Debug, Default, Display, From, Into, PartialEq, Eq, Hash, Serialize, Deserialize,
)]
pub struct SensorId(pub u16);

#[derive(
    Clone, Debug, Default, Display, From, Into, PartialEq, Eq, Hash, Serialize, Deserialize,
)]
pub struct DriverId(pub String);

#[derive(
    Clone, Copy, Debug, Default, Display, From, Into, PartialEq, Eq, Hash, Serialize, Deserialize,
)]
pub struct MessageSeq(pub u32);

impl MessageSeq {
    pub fn wrapping_next(self) -> Self {
        let value: u32 = self.into();
        Self(value.wrapping_add(1))
    }
}

#[derive(
    Clone, Copy, Debug, Default, Display, From, Into, PartialEq, Eq, Hash, Serialize, Deserialize,
)]
pub struct TimestampUs(pub u64);

impl TimestampUs {
    pub fn elapsed_since(self, earlier: Self) -> u64 {
        let now: u64 = self.into();
        let earlier: u64 = earlier.into();
        now.saturating_sub(earlier)
    }

    pub fn seconds_f64(self) -> f64 {
        let value: u64 = self.into();
        value as f64 / 1_000_000.0
    }
}

#[derive(
    Clone, Copy, Debug, Default, Display, From, Into, PartialEq, Eq, Hash, Serialize, Deserialize,
)]
pub struct SampleIndex(pub u32);

impl SampleIndex {
    pub fn wrapping_next(self) -> Self {
        let value: u32 = self.into();
        Self(value.wrapping_add(1))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ImuId {
    /// Identifies the device or board that owns this IMU.
    pub system_id: SystemId,
    /// Identifies one IMU within the owning system.
    pub sensor_id: SensorId,
}

#[derive(
    Clone, Copy, Debug, Default, Display, From, Into, PartialEq, Eq, Hash, Serialize, Deserialize,
)]
/// Identifies one physical or logical bus within a system.
pub struct BusId(pub u8);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemInfo {
    pub system_id: SystemId,
    pub label: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerSource {
    #[default]
    Unknown,
    Battery,
    Usb,
    External,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatteryChargeState {
    #[default]
    Unknown,
    NotCharging,
    Charging,
    Discharging,
    Full,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatteryStatus {
    pub voltage_mv: Option<u16>,
    pub percentage: Option<u8>,
    pub temperature_deci_c: Option<i16>,
    pub charge_state: BatteryChargeState,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PowerStatus {
    pub source: PowerSource,
    pub battery: Option<BatteryStatus>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LowPowerSeverity {
    #[default]
    Low,
    Critical,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BusInfo {
    pub bus_id: BusId,
    pub label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImuChipModel {
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

/// Six-axis sample configuration capability can be independent dimensions or explicitly
/// constrained valid tuples.
///
/// Current implemented capabilities:
/// - ICM-42688-HXY: independent accel 4/8/16 g, gyro 250/500/1000/2000 dps, 100 Hz.
/// - SC7I22/SC7U22: independent accel 4/8/16 g, gyro 250/500/1000 dps, 100 Hz.
/// - ICM-42688-PC: independent accel 2 g, gyro 2048 dps, 100 Hz.
/// - QMI8658A: independent accel 2 g, gyro 2048 dps, 100 Hz.
///
/// Temperature is intentionally modeled separately because most IMU
/// temperature channels do not share the accel/gyro range and ODR choices.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SampleConfigCapability {
    Independent {
        accel_ranges: Cow<'static, [RangeG]>,
        gyro_ranges: Cow<'static, [RangeDps]>,
        sample_rates: Cow<'static, [SampleRateHz]>,
    },
    Constrained {
        configs: Cow<'static, [ImuSampleConfig]>,
    },
}

impl SampleConfigCapability {
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImuChipProfile {
    pub model: ImuChipModel,
    pub sample_config_capability: SampleConfigCapability,
    pub sensor_timestamp: bool,
    pub temperature_scale: Option<TemperatureScale>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImuIdentity {
    pub who_am_i: u8,
    pub revision: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DetectedChipInfo {
    pub chip_profile: ImuChipProfile,
    pub identity: ImuIdentity,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImuDeviceInfo {
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
