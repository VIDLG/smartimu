use crate::bus::SpiProfile;
use crate::error::ImuError;
use crate::driver::ImuDriver;
use crate::types::ImuSampleConfig;
use alloc::vec::Vec;

pub struct DriverDescriptor {
    pub name: &'static str,
    pub driver: &'static dyn ImuDriver,
}

#[derive(Clone, Copy)]
pub struct CandidateDriver {
    pub descriptor: &'static DriverDescriptor,
    pub profiles: &'static [SpiProfile],
}

pub fn ensure_supported_sample_config(
    supported_sample_configs: Vec<ImuSampleConfig>,
    config: &ImuSampleConfig,
) -> Result<(), ImuError> {
    if supported_sample_configs.contains(config) {
        Ok(())
    } else {
        Err(ImuError::UnsupportedConfig)
    }
}

pub mod bmi270;
pub mod hxy42688;
pub mod icm42688;
pub mod lsm6;
pub mod qmi8658;
