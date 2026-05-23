use alloc::string::String;
use alloc::vec::Vec;
use crate::bus::{SpiProfile, ImuTargetId};
use crate::drivers::CandidateDriver;
use crate::protocol::BusDescriptor;
use crate::types::ImuDescriptor;

pub const MAX_DEVICE_IMUS: usize = 16;

#[derive(Clone)]
pub struct ImuInstanceProfile {
    pub descriptor: ImuDescriptor,
    pub target: ImuTargetId,
    pub candidates: &'static [CandidateDriver],
    pub default_profiles: &'static [SpiProfile],
}

pub struct DeviceProfile {
    pub system_id: u16,
    pub system_label: String,
    pub buses: Vec<BusDescriptor>,
    pub imus: Vec<ImuInstanceProfile>,
}
