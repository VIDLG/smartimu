use crate::types::{ImuChip, ImuSampleConfig};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawSample {
    pub accel: [i16; 3],
    pub gyro: [i16; 3],
    pub temp: Option<i16>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PhysicalSample {
    pub accel_g: [f32; 3],
    pub gyro_dps: [f32; 3],
    pub temp_c: Option<f32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ScaleProfile {
    pub accel_g_per_lsb: f32,
    pub gyro_dps_per_lsb: f32,
    pub temp_c_per_lsb: Option<f32>,
    pub temp_offset_c: f32,
}

impl RawSample {
    pub fn to_physical(self, scale: ScaleProfile) -> PhysicalSample {
        PhysicalSample {
            accel_g: self.accel.map(|value| value as f32 * scale.accel_g_per_lsb),
            gyro_dps: self.gyro.map(|value| value as f32 * scale.gyro_dps_per_lsb),
            temp_c: match (self.temp, scale.temp_c_per_lsb) {
                (Some(raw), Some(factor)) => Some(raw as f32 * factor + scale.temp_offset_c),
                _ => None,
            },
        }
    }
}

pub fn default_scale_profile_for_chip(chip: ImuChip) -> Option<ScaleProfile> {
    let profile = match chip {
        ImuChip::Unknown => return None,
        ImuChip::Icm42688Hxy => ScaleProfile {
            accel_g_per_lsb: 1.0 / 4096.0,
            gyro_dps_per_lsb: 1.0 / 16.4,
            temp_c_per_lsb: None,
            temp_offset_c: 0.0,
        },
        ImuChip::Icm42688Pc => ScaleProfile {
            accel_g_per_lsb: 1.0 / 16384.0,
            gyro_dps_per_lsb: 1.0 / 16.0,
            temp_c_per_lsb: None,
            temp_offset_c: 0.0,
        },
        ImuChip::Bmi270 => ScaleProfile {
            accel_g_per_lsb: 1.0 / 2048.0,
            gyro_dps_per_lsb: 1.0 / 16.4,
            temp_c_per_lsb: None,
            temp_offset_c: 0.0,
        },
        ImuChip::Qmi8658A => ScaleProfile {
            accel_g_per_lsb: 1.0 / 16384.0,
            gyro_dps_per_lsb: 1.0 / 16.0,
            temp_c_per_lsb: None,
            temp_offset_c: 0.0,
        },
        ImuChip::Sc7u22 => ScaleProfile {
            accel_g_per_lsb: 1.0 / 4096.0,
            gyro_dps_per_lsb: 500.0 / 32768.0,
            temp_c_per_lsb: None,
            temp_offset_c: 0.0,
        },
    };

    Some(profile)
}

pub fn scale_profile_for_config(chip: ImuChip, config: &ImuSampleConfig) -> Option<ScaleProfile> {
    if chip == ImuChip::Unknown {
        return None;
    }

    Some(ScaleProfile {
        accel_g_per_lsb: config.accel_range.0 as f32 / 32768.0,
        gyro_dps_per_lsb: config.gyro_range.0 as f32 / 32768.0,
        temp_c_per_lsb: None,
        temp_offset_c: 0.0,
    })
}
