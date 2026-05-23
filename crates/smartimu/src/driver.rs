use crate::bus::{ImuBus, ImuTargetId, SpiProfile};
use crate::error::ImuError;
use crate::resource::DriverResources;
use crate::sample::{RawSample, ScaleProfile};
use crate::types::{ImuChip, ImuSampleConfig};
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImuTargetInfo {
    pub id: crate::types::ImuId,
    pub target: ImuTargetId,
}

pub trait ImuDriver: Sync {
    fn chip(&self) -> ImuChip;
    fn probe(&self, bus: &mut dyn ImuBus<Profile = SpiProfile>, target: ImuTargetId) -> Result<bool, ImuError>;
    fn reset(&self, bus: &mut dyn ImuBus<Profile = SpiProfile>, target: ImuTargetId) -> Result<(), ImuError>;
    fn configure(
        &self,
        bus: &mut dyn ImuBus<Profile = SpiProfile>,
        target: ImuTargetId,
        config: &ImuSampleConfig,
        resources: &dyn DriverResources,
    ) -> Result<(), ImuError>;
    fn read_raw(&self, bus: &mut dyn ImuBus<Profile = SpiProfile>, target: ImuTargetId) -> Result<RawSample, ImuError>;
    fn scale_profile(&self) -> ScaleProfile;
    fn supported_sample_configs(&self) -> Vec<ImuSampleConfig>;
}
