#![no_std]

extern crate alloc;

pub mod bus;
pub mod driver;
pub mod error;
pub mod protocol;
pub mod resource;
pub mod sample;
pub mod types;

#[cfg(feature = "esp")]
pub mod drivers;
#[cfg(feature = "esp")]
pub mod firmware;
#[cfg(feature = "esp")]
pub mod platform;
pub mod fusion;

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

#[cfg(feature = "esp")]
pub use drivers::{CandidateDriver, DriverDescriptor};
#[cfg(feature = "esp")]
pub use firmware::device::{DeviceProfile, ImuInstanceProfile, MAX_DEVICE_IMUS};
#[cfg(feature = "esp")]
pub use firmware::resources::EmptyResources;
#[cfg(feature = "esp")]
pub use firmware::runtime::probe_first_matching;
#[cfg(feature = "esp")]
pub use firmware::transport::{SessionRuntime, bounded_string, protocol_string};
#[cfg(feature = "esp")]
pub use platform::bus::{EspImuBus, delay_ms};
#[cfg(feature = "esp")]
pub use platform::resources::EspDriverResources;
