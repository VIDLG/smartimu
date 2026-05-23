use serde::{Deserialize, Serialize};

use crate::error::SmartImuError;
use crate::types::BusId;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
/// Address of one IMU target on a bus.
pub struct ImuTargetId {
    /// Bus that carries this target.
    pub bus_id: BusId,
    /// Board-defined target slot on that bus, such as a chip-select index.
    pub target_index: u8,
}

/// SPI read turnaround cycles :how many dummy bytes to transmit between
/// the register address and the data phase.
///
/// `Turnaround(0)` = data starts immediately after the address byte.
/// `Turnaround(1)` = one dummy byte before reading the response.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Turnaround(pub u8);

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

pub trait ImuBus<Profile = SpiProfile> {
    fn apply_profile(&mut self, target: ImuTargetId, profile: Profile)
    -> Result<(), SmartImuError>;
    fn write_regs(
        &mut self,
        target: ImuTargetId,
        reg: u8,
        data: &[u8],
    ) -> Result<(), SmartImuError>;
    fn read_regs(
        &mut self,
        target: ImuTargetId,
        reg: u8,
        turnaround: Turnaround,
        data: &mut [u8],
    ) -> Result<(), SmartImuError>;
    fn write_reg(&mut self, target: ImuTargetId, reg: u8, value: u8) -> Result<(), SmartImuError> {
        self.write_regs(target, reg, &[value])
    }

    fn read_reg(
        &mut self,
        target: ImuTargetId,
        reg: u8,
        turnaround: Turnaround,
    ) -> Result<u8, SmartImuError> {
        let mut data = [0u8; 1];
        self.read_regs(target, reg, turnaround, &mut data)?;
        Ok(data[0])
    }
}
