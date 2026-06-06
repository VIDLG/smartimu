#![no_std]

extern crate alloc;

pub mod bus;
pub mod delay;
#[cfg(feature = "esp")]
pub mod device;
pub mod driver;
pub mod error;
#[cfg(feature = "esp")]
pub mod probe;
pub mod protocol;
pub mod sample;
#[cfg(feature = "esp")]
pub mod transport;
pub mod types;

#[cfg(feature = "esp")]
pub mod drivers;
pub mod fusion;
#[cfg(feature = "esp")]
pub mod platform;

pub use bus::{ImuBus, ImuTargetId, SpiMode, SpiProfile, Turnaround};
pub use driver::{
    DataReadyCondition, DataReadyStatus, DriverInfo, ImuDriver, ImuTargetInfo, ProbeRegisterMatch,
    ProbeRegisterReadout, SampleByteOrder, SampleRegisterReadout, ensure_sample_config_allowed,
    ensure_sample_readout_allowed,
};
pub use error::{SmartImuError, SmartImuResult, UnsupportedConfigReason};
pub use protocol::*;
pub use sample::{
    Imu6Scale, ImuSampleScale, PhysicalImu6, PhysicalImuSample, PhysicalTemperature, RawImu6,
    RawImuSample, RawTemperature, SensorTimestamp,
};
pub use types::{
    BusId, BusInfo, DetectedChipInfo, ImuChip, ImuChipProfile, ImuId, ImuIdentity, ImuNodeInfo,
    ImuSampleConfig, Quaternion, RangeDps, RangeG, SampleConfigCapability, SampleRateHz,
    SystemInfo, TemperatureScale,
};

pub use delay::delay_ms;
#[cfg(feature = "esp")]
pub use device::{ConfiguredImuDevice, DetectedImuDevice};
#[cfg(feature = "esp")]
pub use platform::bus::EspImuBus;
#[cfg(feature = "esp")]
pub use probe::{
    CandidateDriver, ProbeMatch, ProbePlan, probe, probe_driver, probe_first_matching,
};
#[cfg(feature = "esp")]
pub use transport::{SessionRuntime, bounded_string, protocol_string};
