use serde::{Deserialize, Serialize};

use crate::error::ImuError;
use crate::types::BusId;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
/// Address of one IMU target on a bus.
pub struct ImuTargetId {
    /// Bus that carries this target.
    pub bus_id: BusId,
    /// Board-defined target slot on that bus, such as a chip-select index.
    pub target_index: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// SPI clock mode used when talking to a target.
pub enum SpiMode {
    /// CPOL = 0, CPHA = 0.
    Mode0,
    /// CPOL = 0, CPHA = 1.
    Mode1,
    /// CPOL = 1, CPHA = 0.
    Mode2,
    /// CPOL = 1, CPHA = 1.
    Mode3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpiProfile {
    pub id: u8,
    pub mode: SpiMode,
    pub frequency_khz: u32,
}

impl SpiProfile {
    pub const fn new(id: u8, mode: SpiMode, frequency_khz: u32) -> Self {
        Self {
            id,
            mode,
            frequency_khz,
        }
    }
}

pub trait ImuBus {
    type Profile;
    fn apply_profile(
        &mut self,
        target: ImuTargetId,
        profile: Self::Profile,
    ) -> Result<(), ImuError>;
    fn write_regs(&mut self, target: ImuTargetId, reg: u8, data: &[u8]) -> Result<(), ImuError>;
    fn read_regs(
        &mut self,
        target: ImuTargetId,
        reg: u8,
        dummy_bytes: usize,
        data: &mut [u8],
    ) -> Result<(), ImuError>;
    fn delay_ms(&mut self, ms: u64);

    fn write_reg(&mut self, target: ImuTargetId, reg: u8, value: u8) -> Result<(), ImuError> {
        self.write_regs(target, reg, &[value])
    }

    fn read_reg(
        &mut self,
        target: ImuTargetId,
        reg: u8,
        dummy_bytes: usize,
    ) -> Result<u8, ImuError> {
        let mut data = [0u8; 1];
        self.read_regs(target, reg, dummy_bytes, &mut data)?;
        Ok(data[0])
    }
}
