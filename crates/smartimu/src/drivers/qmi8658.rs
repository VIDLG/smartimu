use crate::{
    DriverResources, ImuBus, ImuChip, ImuDriver, ImuError, ImuSampleConfig, ImuTargetId, RangeDps,
    RangeG, RawSample, SampleRateHz, ScaleProfile, SpiProfile,
};

const CHIP_ID: u8 = 0x05;
const CHIP_ID_ALT: u8 = 0x3E;

const REG_WHO_AM_I: u8 = 0x00;
const REG_REVISION_ID: u8 = 0x01;
const REG_CTRL1: u8 = 0x02;
const REG_CTRL2: u8 = 0x03;
const REG_CTRL3: u8 = 0x04;
const REG_CTRL5: u8 = 0x06;
const REG_CTRL7: u8 = 0x08;
const REG_STATUS0: u8 = 0x2E;
const REG_AX_L: u8 = 0x35;
const REG_RESET: u8 = 0x60;

pub static DRIVER: Qmi8658Driver = Qmi8658Driver;
pub static DESCRIPTOR: super::DriverDescriptor = super::DriverDescriptor {
    name: "QMI8658A",
    driver: &DRIVER,
};

pub struct Qmi8658Driver;

impl ImuDriver for Qmi8658Driver {
    fn chip(&self) -> ImuChip {
        ImuChip::Qmi8658A
    }

    fn probe(&self, bus: &mut dyn ImuBus<Profile = SpiProfile>, target: ImuTargetId) -> Result<bool, ImuError> {
        for _ in 0..3 {
            let id = bus.read_reg(target, REG_WHO_AM_I, 0)?;
            let revision = bus.read_reg(target, REG_REVISION_ID, 0)?;
            if id == CHIP_ID || (id == CHIP_ID_ALT && revision == CHIP_ID_ALT) {
                return Ok(true);
            }
            bus.delay_ms(5);
        }
        Ok(false)
    }

    fn reset(&self, bus: &mut dyn ImuBus<Profile = SpiProfile>, target: ImuTargetId) -> Result<(), ImuError> {
        bus.write_reg(target, REG_RESET, 0xB0)?;
        bus.delay_ms(20);
        Ok(())
    }

    fn configure(
        &self,
        bus: &mut dyn ImuBus<Profile = SpiProfile>,
        target: ImuTargetId,
        config: &ImuSampleConfig,
        _resources: &dyn DriverResources,
    ) -> Result<(), ImuError> {
        super::ensure_supported_sample_config(self.supported_sample_configs(), config)?;
        bus.write_reg(target, REG_CTRL1, 0x20)?;
        bus.write_reg(target, REG_CTRL2, 0x06)?;
        bus.write_reg(target, REG_CTRL3, 0x76)?;
        bus.write_reg(target, REG_CTRL5, 0x00)?;
        bus.write_reg(target, REG_CTRL7, 0x03)?;
        bus.delay_ms(50);
        Ok(())
    }

    fn read_raw(&self, bus: &mut dyn ImuBus<Profile = SpiProfile>, target: ImuTargetId) -> Result<RawSample, ImuError> {
        for _ in 0..10 {
            let status = bus.read_reg(target, REG_STATUS0, 0)?;
            if status & 0x03 == 0x03 {
                return read_sample(bus, target);
            }
            bus.delay_ms(1);
        }
        read_sample(bus, target)
    }

    fn scale_profile(&self) -> ScaleProfile {
        ScaleProfile {
            accel_g_per_lsb: 1.0 / 16384.0,
            gyro_dps_per_lsb: 1.0 / 16.0,
            temp_c_per_lsb: None,
            temp_offset_c: 0.0,
        }
    }

    fn supported_sample_configs(&self) -> alloc::vec::Vec<ImuSampleConfig> {
        alloc::vec![ImuSampleConfig {
            accel_range: RangeG(2),
            gyro_range: RangeDps(2048),
            sample_rate_hz: SampleRateHz(100),
        }]
    }
}

fn read_sample(bus: &mut dyn ImuBus<Profile = SpiProfile>, target: ImuTargetId) -> Result<RawSample, ImuError> {
    Ok(RawSample {
        accel: [
            read_i16_le(bus, target, REG_AX_L)?,
            read_i16_le(bus, target, REG_AX_L + 2)?,
            read_i16_le(bus, target, REG_AX_L + 4)?,
        ],
        gyro: [
            read_i16_le(bus, target, REG_AX_L + 6)?,
            read_i16_le(bus, target, REG_AX_L + 8)?,
            read_i16_le(bus, target, REG_AX_L + 10)?,
        ],
        temp: None,
    })
}

fn read_i16_le(bus: &mut dyn ImuBus<Profile = SpiProfile>, target: ImuTargetId, low_reg: u8) -> Result<i16, ImuError> {
    let low = bus.read_reg(target, low_reg, 0)?;
    let high = bus.read_reg(target, low_reg + 1, 0)?;
    Ok(i16::from_le_bytes([low, high]))
}
