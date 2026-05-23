use embassy_time::{Duration, block_for};
use embedded_hal::spi::SpiBus as EmbeddedSpiBus;
use esp_hal::Blocking;
use esp_hal::gpio::Output;
use esp_hal::spi::master::{Config, Spi};
use esp_hal::time::Rate;
use crate::{SpiMode, SpiProfile, ImuBus, ImuError, ImuTargetId};

const MAX_WRITE_BYTES: usize = 40;
const MAX_READ_BYTES: usize = 64;

pub struct EspImuBus<'a, 'd, const N: usize> {
    spi: &'a mut Spi<'d, Blocking>,
    targets: [ImuTargetId; N],
    chip_selects: [Output<'d>; N],
}

impl<'a, 'd, const N: usize> EspImuBus<'a, 'd, N> {
    pub fn new(
        spi: &'a mut Spi<'d, Blocking>,
        targets: [ImuTargetId; N],
        chip_selects: [Output<'d>; N],
    ) -> Self {
        Self {
            spi,
            targets,
            chip_selects,
        }
    }

    fn index_for(&self, target: ImuTargetId) -> Result<usize, ImuError> {
        self.targets
            .iter()
            .position(|candidate| *candidate == target)
            .ok_or(ImuError::InvalidTarget)
    }
}

impl From<SpiMode> for esp_hal::spi::Mode {
    fn from(mode: SpiMode) -> Self {
        match mode {
            SpiMode::Mode0 => esp_hal::spi::Mode::_0,
            SpiMode::Mode1 => esp_hal::spi::Mode::_1,
            SpiMode::Mode2 => esp_hal::spi::Mode::_2,
            SpiMode::Mode3 => esp_hal::spi::Mode::_3,
        }
    }
}

impl<const N: usize> ImuBus for EspImuBus<'_, '_, N> {
    type Profile = SpiProfile;

    fn apply_profile(&mut self, _target: ImuTargetId, profile: SpiProfile) -> Result<(), ImuError> {
        self.spi
            .apply_config(
                &Config::default()
                    .with_frequency(Rate::from_khz(profile.frequency_khz))
                    .with_mode(profile.mode.into()),
            )
            .map_err(|_| ImuError::ConfigError)
    }

    fn write_regs(&mut self, target: ImuTargetId, reg: u8, data: &[u8]) -> Result<(), ImuError> {
        let total = 1 + data.len();
        if total > MAX_WRITE_BYTES {
            return Err(ImuError::ConfigError);
        }

        let mut buf = [0u8; MAX_WRITE_BYTES];
        buf[0] = reg & 0x7F;
        buf[1..total].copy_from_slice(data);

        let index = self.index_for(target)?;
        self.chip_selects[index].set_low();
        let result = self
            .spi
            .write(&buf[..total])
            .and_then(|_| self.spi.flush())
            .map_err(|_| ImuError::CommunicationError);
        self.chip_selects[index].set_high();
        result
    }

    fn read_regs(
        &mut self,
        target: ImuTargetId,
        reg: u8,
        dummy_bytes: usize,
        data: &mut [u8],
    ) -> Result<(), ImuError> {
        let total = 1 + dummy_bytes + data.len();
        if total > MAX_READ_BYTES {
            return Err(ImuError::ConfigError);
        }

        let mut buf = [0u8; MAX_READ_BYTES];
        buf[0] = reg | 0x80;

        let index = self.index_for(target)?;
        self.chip_selects[index].set_low();
        let result = self
            .spi
            .transfer_in_place(&mut buf[..total])
            .and_then(|_| self.spi.flush())
            .map_err(|_| ImuError::CommunicationError);
        self.chip_selects[index].set_high();

        result?;
        let start = 1 + dummy_bytes;
        data.copy_from_slice(&buf[start..start + data.len()]);
        Ok(())
    }

    fn delay_ms(&mut self, ms: u64) {
        block_for(Duration::from_millis(ms));
    }
}
