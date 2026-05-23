#![no_std]

extern crate alloc;

pub mod bus;
pub mod driver;
pub mod drivers;
pub mod error;
pub mod firmware;
pub mod fusion;
pub mod platform;
pub mod protocol;
pub mod resource;
pub mod sample;
pub mod types;

pub use bus::{SpiMode, SpiProfile, ImuBus, ImuTargetId};
pub use driver::{ImuDriver, ImuTargetInfo};
pub use drivers::{CandidateDriver, DriverDescriptor};
pub use error::ImuError;
pub use firmware::device::{DeviceProfile, ImuInstanceProfile, MAX_DEVICE_IMUS};
pub use firmware::resources::EmptyResources;
pub use firmware::runtime::probe_first_matching;
pub use firmware::transport::{SessionRuntime, bounded_string, protocol_string};
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

#[cfg(feature = "esp")]
pub use platform::bus::EspImuBus;
#[cfg(feature = "esp")]
pub use platform::resources::EspDriverResources;
