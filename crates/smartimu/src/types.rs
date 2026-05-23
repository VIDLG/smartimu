use alloc::string::String;
use alloc::vec::Vec;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImuChip {
    Unknown,
    Icm42688Hxy,
    Icm42688Pc,
    Bmi270, // broken, not supported
    Qmi8658A,
    Sc7u22,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RangeG(pub u16);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RangeDps(pub u16);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SampleRateHz(pub u16);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImuSampleConfig {
    pub accel_range: RangeG,
    pub gyro_range: RangeDps,
    pub sample_rate_hz: SampleRateHz,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImuDescriptor {
    pub id: ImuId,
    pub bus_id: BusId,
    pub chip: ImuChip,
    pub label: String,
    pub sample_config: Option<ImuSampleConfig>,
    pub supported_sample_configs: Vec<ImuSampleConfig>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Quaternion {
    pub w: f32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}
