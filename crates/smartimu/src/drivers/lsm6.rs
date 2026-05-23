use crate::{
    delay_ms, DriverResources, ImuBus, ImuChip, ImuDriver, ImuError, ImuSampleConfig,
    ImuTargetId, RangeDps, RangeG, RawSample, SampleRateHz, ScaleProfile, SpiProfile,
};

const CHIP_ID: u8 = 0x6A;
const REG_WHO_AM_I: u8 = 0x01;
const REG_COM_CFG: u8 = 0x05;
const REG_DATA_STAT: u8 = 0x0B;
const REG_ACC_XH: u8 = 0x0C;
const REG_ACC_CONF: u8 = 0x40;
const REG_ACC_RANGE: u8 = 0x41;
const REG_GYR_CONF: u8 = 0x42;
const REG_GYR_RANGE: u8 = 0x43;
const REG_PWR_CTRL: u8 = 0x7D;

pub static DRIVER: Lsm6Driver = Lsm6Driver;
pub static DESCRIPTOR: super::DriverDescriptor = super::DriverDescriptor {
    name: "SC7I22",
    driver: &DRIVER,
};

pub struct Lsm6Driver;

impl ImuDriver for Lsm6Driver {
    fn chip(&self) -> ImuChip {
        ImuChip::Sc7u22
    }

    fn probe(&self, bus: &mut dyn ImuBus<Profile = SpiProfile>, target: ImuTargetId) -> Result<bool, ImuError> {
        for _ in 0..3 {
            let id = bus.read_reg(target, REG_WHO_AM_I, 0)?;
            let com_cfg = bus.read_reg(target, REG_COM_CFG, 0)?;
            if id == CHIP_ID && com_cfg != 0x50 {
                return Ok(true);
            }
            delay_ms(5);
        }
        Ok(false)
    }

    fn reset(&self, _bus: &mut dyn ImuBus<Profile = SpiProfile>, _target: ImuTargetId) -> Result<(), ImuError> {
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
        bus.write_reg(target, REG_PWR_CTRL, 0x0E)?;
        delay_ms(10);
        bus.write_reg(target, REG_ACC_CONF, 0xA8)?;
        bus.write_reg(target, REG_ACC_RANGE, accel_range_reg(config.accel_range)?)?;
        bus.write_reg(target, REG_GYR_CONF, 0xA8)?;
        bus.write_reg(target, REG_GYR_RANGE, gyro_range_reg(config.gyro_range)?)?;
        delay_ms(5);
        Ok(())
    }

    fn read_raw(&self, bus: &mut dyn ImuBus<Profile = SpiProfile>, target: ImuTargetId) -> Result<RawSample, ImuError> {
        let status = bus.read_reg(target, REG_DATA_STAT, 0)?;
        if status & 0x03 == 0 {
            return Err(ImuError::DataNotReady);
        }

        let mut buf = [0u8; 12];
        bus.read_regs(target, REG_ACC_XH, 0, &mut buf)?;
        Ok(RawSample {
            accel: [
                i16::from_be_bytes([buf[0], buf[1]]),
                i16::from_be_bytes([buf[2], buf[3]]),
                i16::from_be_bytes([buf[4], buf[5]]),
            ],
            gyro: [
                i16::from_be_bytes([buf[6], buf[7]]),
                i16::from_be_bytes([buf[8], buf[9]]),
                i16::from_be_bytes([buf[10], buf[11]]),
            ],
            temp: None,
        })
    }

    fn scale_profile(&self) -> ScaleProfile {
        ScaleProfile {
            accel_g_per_lsb: 1.0 / 4096.0,
            gyro_dps_per_lsb: 500.0 / 32768.0,
            temp_c_per_lsb: None,
            temp_offset_c: 0.0,
        }
    }

    fn supported_sample_configs(&self) -> alloc::vec::Vec<ImuSampleConfig> {
        supported_sample_configs(
            &[RangeG(4), RangeG(8), RangeG(16)],
            &[RangeDps(250), RangeDps(500), RangeDps(1000)],
            &[SampleRateHz(100)],
        )
    }
}

fn accel_range_reg(range: RangeG) -> Result<u8, ImuError> {
    match range {
        RangeG(4) => Ok(0x01),
        RangeG(8) => Ok(0x02),
        RangeG(16) => Ok(0x03),
        _ => Err(ImuError::UnsupportedConfig),
    }
}

fn gyro_range_reg(range: RangeDps) -> Result<u8, ImuError> {
    match range {
        RangeDps(1000) => Ok(0x01),
        RangeDps(500) => Ok(0x02),
        RangeDps(250) => Ok(0x03),
        _ => Err(ImuError::UnsupportedConfig),
    }
}

fn supported_sample_configs(
    accel_ranges: &[RangeG],
    gyro_ranges: &[RangeDps],
    sample_rates: &[SampleRateHz],
) -> alloc::vec::Vec<ImuSampleConfig> {
    let mut configs = alloc::vec::Vec::new();
    for accel_range in accel_ranges {
        for gyro_range in gyro_ranges {
            for sample_rate_hz in sample_rates {
                configs.push(ImuSampleConfig {
                    accel_range: *accel_range,
                    gyro_range: *gyro_range,
                    sample_rate_hz: *sample_rate_hz,
                });
            }
        }
    }
    configs
}
