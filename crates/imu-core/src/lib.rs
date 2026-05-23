#![no_std]

extern crate alloc;

pub mod bus;
pub mod driver;
pub mod error;
pub mod protocol;
pub mod resource;
pub mod sample;
pub mod types;

pub use bus::{SpiMode, SpiProfile, ImuBus, ImuTargetId};
pub use driver::{ImuDriver, ImuTargetInfo};
pub use error::ImuError;
pub use protocol::*;
pub use resource::{DriverResourceKey, DriverResources};
pub use sample::{
    PhysicalSample, RawSample, ScaleProfile, default_scale_profile_for_chip,
    scale_profile_for_config,
};
pub use types::{
    BusId, ImuChip, ImuDescriptor, ImuId, ImuSampleConfig, Quaternion, RangeDps, RangeG,
    SampleRateHz,
};
