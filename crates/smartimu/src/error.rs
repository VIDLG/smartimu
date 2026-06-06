use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type SmartImuResult<T> = Result<T, SmartImuError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnsupportedConfigReason {
    SampleConfig,
    AccelRange,
    GyroRange,
    TemperatureReadout,
    SensorTimestampReadout,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Error)]
pub enum SmartImuError {
    #[error("communication error")]
    CommunicationError,
    #[error("chip not found")]
    ChipNotFound,
    #[error("IMU not found")]
    ImuNotFound,
    #[error("configuration error")]
    ConfigError,
    #[error("data not ready")]
    DataNotReady,
    #[error("missing resource")]
    MissingResource,
    #[error("unsupported configuration: {0:?}")]
    UnsupportedConfig(UnsupportedConfigReason),
    #[error("invalid target")]
    InvalidTarget,
}

#[cfg(feature = "esp")]
impl From<esp_hal::spi::Error> for SmartImuError {
    fn from(_: esp_hal::spi::Error) -> Self {
        SmartImuError::CommunicationError
    }
}

#[cfg(feature = "esp")]
impl From<esp_hal::spi::master::ConfigError> for SmartImuError {
    fn from(_: esp_hal::spi::master::ConfigError) -> Self {
        SmartImuError::ConfigError
    }
}
