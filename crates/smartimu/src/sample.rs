use crate::types::{ImuSampleConfig, TemperatureScale};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawImu6 {
    pub accel: [i16; 3],
    pub gyro: [i16; 3],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawTemperature {
    pub raw: i16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SensorTimestamp {
    pub ticks: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawImuSample {
    pub imu6: RawImu6,
    pub temperature: Option<RawTemperature>,
    pub sensor_timestamp: Option<SensorTimestamp>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PhysicalImu6 {
    pub accel_g: [f32; 3],
    pub gyro_dps: [f32; 3],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PhysicalTemperature {
    pub celsius: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PhysicalImuSample {
    pub imu6: PhysicalImu6,
    pub temperature: Option<PhysicalTemperature>,
    pub sensor_timestamp: Option<SensorTimestamp>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Imu6Scale {
    pub accel_g_per_lsb: f32,
    pub gyro_dps_per_lsb: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ImuSampleScale {
    pub imu6: Imu6Scale,
    pub temperature: Option<TemperatureScale>,
}

impl RawImu6 {
    pub fn to_physical(self, scale: Imu6Scale) -> PhysicalImu6 {
        PhysicalImu6 {
            accel_g: self.accel.map(|value| value as f32 * scale.accel_g_per_lsb),
            gyro_dps: self.gyro.map(|value| value as f32 * scale.gyro_dps_per_lsb),
        }
    }
}

impl From<RawImu6> for RawImuSample {
    fn from(imu6: RawImu6) -> Self {
        Self {
            imu6,
            temperature: None,
            sensor_timestamp: None,
        }
    }
}

impl RawTemperature {
    pub fn to_physical(self, scale: TemperatureScale) -> PhysicalTemperature {
        PhysicalTemperature {
            celsius: self.raw as f32 * scale.c_per_lsb + scale.offset_c,
        }
    }
}

impl RawImuSample {
    pub fn to_physical(self, scale: ImuSampleScale) -> PhysicalImuSample {
        PhysicalImuSample {
            imu6: self.imu6.to_physical(scale.imu6),
            temperature: match (self.temperature, scale.temperature) {
                (Some(temperature), Some(scale)) => Some(temperature.to_physical(scale)),
                _ => None,
            },
            sensor_timestamp: self.sensor_timestamp,
        }
    }
}

impl From<ImuSampleConfig> for Imu6Scale {
    fn from(config: ImuSampleConfig) -> Self {
        Self {
            accel_g_per_lsb: config.accel_range.0 as f32 / 32768.0,
            gyro_dps_per_lsb: config.gyro_range.0 as f32 / 32768.0,
        }
    }
}
