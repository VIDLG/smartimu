use crate::{ImuBus, ImuTargetId, SmartImuError, SpiMode, SpiProfile, Turnaround};
use embedded_hal::spi::SpiBus;
use esp_hal::Blocking;
use esp_hal::gpio::Output;
use esp_hal::spi::master::{Config, Spi};
use esp_hal::time::Rate;
use hashbrown::HashMap;

const MAX_WRITE_BYTES: usize = 40;
const MAX_READ_BYTES: usize = 64;

pub struct EspImuBus<'a, 'd> {
    spi: &'a mut Spi<'d, Blocking>,
    chip_selects: HashMap<ImuTargetId, Output<'d>>,
    write_buf: [u8; MAX_WRITE_BYTES],
    read_buf: [u8; MAX_READ_BYTES],
}

impl<'a, 'd> EspImuBus<'a, 'd> {
    pub fn new(spi: &'a mut Spi<'d, Blocking>) -> Self {
        Self {
            spi,
            chip_selects: HashMap::new(),
            write_buf: [0u8; MAX_WRITE_BYTES],
            read_buf: [0u8; MAX_READ_BYTES],
        }
    }

    pub fn with_target(mut self, target: ImuTargetId, chip_select: Output<'d>) -> Self {
        self.chip_selects.insert(target, chip_select);
        self
    }

    fn set_chip_select(
        &mut self,
        target: ImuTargetId,
        selected: bool,
    ) -> Result<(), SmartImuError> {
        let chip_select = self
            .chip_selects
            .get_mut(&target)
            .ok_or(SmartImuError::InvalidTarget)?;
        if selected {
            chip_select.set_low();
        } else {
            chip_select.set_high();
        }
        Ok(())
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

impl ImuBus for EspImuBus<'_, '_> {
    fn apply_profile(
        &mut self,
        _target: ImuTargetId,
        profile: SpiProfile,
    ) -> Result<(), SmartImuError> {
        self.spi.apply_config(
            &Config::default()
                .with_frequency(Rate::from_khz(profile.frequency_khz))
                .with_mode(profile.mode.into()),
        )?;
        Ok(())
    }

    fn write_regs(
        &mut self,
        target: ImuTargetId,
        reg: u8,
        data: &[u8],
    ) -> Result<(), SmartImuError> {
        let total = 1 + data.len();
        if total > MAX_WRITE_BYTES {
            return Err(SmartImuError::ConfigError);
        }

        self.write_buf[0] = reg & 0x7F;
        self.write_buf[1..total].copy_from_slice(data);

        self.set_chip_select(target, true)?;
        let result: Result<(), SmartImuError> = self
            .spi
            .write(&self.write_buf[..total])
            .and_then(|_| self.spi.flush())
            .map_err(SmartImuError::from);
        self.set_chip_select(target, false)?;
        result
    }

    fn read_regs(
        &mut self,
        target: ImuTargetId,
        reg: u8,
        turnaround: Turnaround,
        data: &mut [u8],
    ) -> Result<(), SmartImuError> {
        let dummy = turnaround.0 as usize;
        let total = 1 + dummy + data.len();
        if total > MAX_READ_BYTES {
            return Err(SmartImuError::ConfigError);
        }

        self.read_buf[0] = reg | 0x80;

        self.set_chip_select(target, true)?;
        let result: Result<(), SmartImuError> = self
            .spi
            .transfer_in_place(&mut self.read_buf[..total])
            .and_then(|_| self.spi.flush())
            .map_err(SmartImuError::from);
        self.set_chip_select(target, false)?;

        result?;
        let start = 1 + dummy;
        data.copy_from_slice(&self.read_buf[start..start + data.len()]);
        Ok(())
    }
}
