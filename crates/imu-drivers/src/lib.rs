#![no_std]

extern crate alloc;

pub mod bmi270;
pub mod hxy42688;
pub mod icm42688;
pub mod lsm6;
pub mod qmi8658;

use imu_core::{SpiProfile, ImuDriver};
use imu_core::{ImuError, ImuSampleConfig};

pub struct DriverDescriptor {
    pub name: &'static str,
    pub driver: &'static dyn ImuDriver,
}

#[derive(Clone, Copy)]
pub struct CandidateDriver {
    pub descriptor: &'static DriverDescriptor,
    pub profiles: &'static [SpiProfile],
}

fn ensure_supported_sample_config(
    supported_sample_configs: alloc::vec::Vec<ImuSampleConfig>,
    config: &ImuSampleConfig,
) -> Result<(), ImuError> {
    if supported_sample_configs.contains(config) {
        Ok(())
    } else {
        Err(ImuError::UnsupportedConfig)
    }
}
