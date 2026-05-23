use imu_core::{
    DriverResourceKey, DriverResources, ImuBus, ImuChip, ImuDriver, ImuError, ImuSampleConfig,
    ImuTargetId, RangeDps, RangeG, RawSample, SampleRateHz, ScaleProfile, SpiProfile,
};

const CHIP_ID: u8 = 0x24;
const BURST_CHUNK: usize = 32;

pub static DRIVER: Bmi270Driver = Bmi270Driver;
pub static DESCRIPTOR: crate::DriverDescriptor = crate::DriverDescriptor {
    name: "BMI270",
    driver: &DRIVER,
};

pub struct Bmi270Driver;

impl ImuDriver for Bmi270Driver {
    fn chip(&self) -> ImuChip {
        ImuChip::Bmi270
    }

    fn probe(&self, bus: &mut dyn ImuBus<Profile = SpiProfile>, target: ImuTargetId) -> Result<bool, ImuError> {
        for _ in 0..3 {
            for dummy in [1usize, 0, 2] {
                let _ = bus.read_reg(target, 0x00, dummy);
                bus.delay_ms(2);
                if let Ok(id) = bus.read_reg(target, 0x00, dummy) {
                    if id == CHIP_ID {
                        return Ok(true);
                    }
                }
            }

            let _ = bus.write_reg(target, 0x7E, 0xB6);
            bus.delay_ms(20);
        }

        Ok(false)
    }

    fn reset(&self, bus: &mut dyn ImuBus<Profile = SpiProfile>, target: ImuTargetId) -> Result<(), ImuError> {
        bus.write_reg(target, 0x7E, 0xB6)?;
        bus.delay_ms(10);
        Ok(())
    }

    fn configure(
        &self,
        bus: &mut dyn ImuBus<Profile = SpiProfile>,
        target: ImuTargetId,
        config: &ImuSampleConfig,
        resources: &dyn DriverResources,
    ) -> Result<(), ImuError> {
        crate::ensure_supported_sample_config(self.supported_sample_configs(), config)?;
        let blob = resources
            .bytes(DriverResourceKey::Bmi270ConfigBlob)
            .ok_or(ImuError::MissingResource)?;

        let _ = bus.read_reg(target, 0x00, 1);
        bus.delay_ms(1);

        let pwr_conf = bus.read_reg(target, 0x7C, 1)?;
        bus.write_reg(target, 0x7C, pwr_conf & !0x01)?;
        bus.delay_ms(1);

        bus.write_reg(target, 0x59, 0x00)?;
        bus.delay_ms(1);

        let mut index = 0;
        while index + BURST_CHUNK <= blob.len() {
            upload_chunk(bus, target, index, &blob[index..index + BURST_CHUNK])?;
            index += BURST_CHUNK;
        }

        while index < blob.len() {
            let remaining = blob.len() - index;
            if remaining >= 2 {
                upload_chunk(bus, target, index, &blob[index..index + 2])?;
                index += 2;
            } else {
                return Err(ImuError::ConfigError);
            }
        }

        bus.write_reg(target, 0x59, 0x01)?;
        bus.delay_ms(150);

        let status = bus.read_reg(target, 0x21, 1)?;
        if status & 0x0F != 0x01 {
            return Err(ImuError::ConfigError);
        }

        bus.write_reg(target, 0x40, 0xA8)?;
        bus.write_reg(target, 0x41, accel_range_reg(config.accel_range)?)?;
        bus.write_reg(target, 0x42, 0xA9)?;
        bus.write_reg(target, 0x43, gyro_range_reg(config.gyro_range)?)?;
        bus.write_reg(target, 0x7D, 0x0E)?;
        bus.delay_ms(50);
        Ok(())
    }

    fn read_raw(&self, bus: &mut dyn ImuBus<Profile = SpiProfile>, target: ImuTargetId) -> Result<RawSample, ImuError> {
        let mut buf = [0u8; 12];
        bus.read_regs(target, 0x0C, 1, &mut buf)?;
        Ok(RawSample {
            accel: [
                i16::from_le_bytes([buf[0], buf[1]]),
                i16::from_le_bytes([buf[2], buf[3]]),
                i16::from_le_bytes([buf[4], buf[5]]),
            ],
            gyro: [
                i16::from_le_bytes([buf[6], buf[7]]),
                i16::from_le_bytes([buf[8], buf[9]]),
                i16::from_le_bytes([buf[10], buf[11]]),
            ],
            temp: None,
        })
    }

    fn scale_profile(&self) -> ScaleProfile {
        ScaleProfile {
            accel_g_per_lsb: 1.0 / 2048.0,
            gyro_dps_per_lsb: 1.0 / 16.4,
            temp_c_per_lsb: None,
            temp_offset_c: 0.0,
        }
    }

    fn supported_sample_configs(&self) -> alloc::vec::Vec<ImuSampleConfig> {
        supported_sample_configs(
            &[RangeG(2), RangeG(4), RangeG(8), RangeG(16)],
            &[RangeDps(125), RangeDps(250), RangeDps(500), RangeDps(2000)],
            &[SampleRateHz(100)],
        )
    }
}

fn upload_chunk(
    bus: &mut dyn ImuBus<Profile = SpiProfile>,
    target: ImuTargetId,
    index: usize,
    data: &[u8],
) -> Result<(), ImuError> {
    let addr: u16 = (index / 2).try_into().map_err(|_| ImuError::ConfigError)?;
    let addr_bytes = [(addr & 0x0F) as u8, (addr >> 4) as u8];
    bus.write_regs(target, 0x5B, &addr_bytes)?;
    bus.write_regs(target, 0x5E, data)
}

fn accel_range_reg(range: RangeG) -> Result<u8, ImuError> {
    match range {
        RangeG(2) => Ok(0x00),
        RangeG(4) => Ok(0x01),
        RangeG(8) => Ok(0x02),
        RangeG(16) => Ok(0x03),
        _ => Err(ImuError::UnsupportedConfig),
    }
}

fn gyro_range_reg(range: RangeDps) -> Result<u8, ImuError> {
    match range {
        RangeDps(2000) => Ok(0x00),
        RangeDps(1000) => Ok(0x01),
        RangeDps(500) => Ok(0x02),
        RangeDps(250) => Ok(0x03),
        RangeDps(125) => Ok(0x04),
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
