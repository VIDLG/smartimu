use crate::{
    DataReadyCondition, DataReadyStatus, DriverInfo, ImuBus, ImuChip, ImuChipProfile, ImuDriver,
    ImuSampleConfig, ImuTargetId, ProbeRegisterMatch, ProbeRegisterReadout, RangeDps, RangeG,
    SampleByteOrder, SampleRateHz, SampleRegisterReadout, SmartImuError, delay_ms,
};
use alloc::{borrow::Cow, boxed::Box};
use async_trait::async_trait;

const CHIP_ID: u8 = 0x6A;
const COM_CFG_DEFAULT: u8 = 0x50;
const PROBE_RETRY_DELAY_MS: u64 = 5;
const SENSOR_POWER_UP_DELAY_MS: u64 = 10;
const CONFIG_SETTLE_DELAY_MS: u64 = 5;

const REG_WHO_AM_I: u8 = 0x01;
const REG_COM_CFG: u8 = 0x05;
const REG_DATA_STAT: u8 = 0x0B;
const REG_ACC_XH: u8 = 0x0C;
const REG_ACC_CONF: u8 = 0x40;
const REG_ACC_RANGE: u8 = 0x41;
const REG_GYR_CONF: u8 = 0x42;
const REG_GYR_RANGE: u8 = 0x43;
const REG_PWR_CTRL: u8 = 0x7D;

const ACCEL_RANGES: &[RangeG] = &[RangeG(4), RangeG(8), RangeG(16)];
const GYRO_RANGES: &[RangeDps] = &[RangeDps(250), RangeDps(500), RangeDps(1000), RangeDps(2000)];
const SAMPLE_RATES: &[SampleRateHz] = &[SampleRateHz(100)];
const PROBE_MATCHES: &[ProbeRegisterMatch] = &[ProbeRegisterMatch::WhoAmIAndRevision {
    who_am_i: CHIP_ID,
    revision: COM_CFG_DEFAULT,
}];

pub static CHIP_PROFILE: ImuChipProfile = ImuChipProfile {
    chip: ImuChip::Icm42688Hxy,
    sample_config_options: crate::SampleConfigOptions::Independent {
        accel_ranges: Cow::Borrowed(ACCEL_RANGES),
        gyro_ranges: Cow::Borrowed(GYRO_RANGES),
        sample_rates: Cow::Borrowed(SAMPLE_RATES),
    },
    sample_readout_support: crate::SampleReadoutSupport {
        temperature: false,
        sensor_timestamp: false,
    },
    temperature_config: None,
};

pub static DRIVER: Hxy42688Driver = Hxy42688Driver;
pub static INFO: crate::DriverInfo = crate::DriverInfo {
    name: "ICM-42688-HXY",
    driver: &DRIVER,
    chip_profile: &CHIP_PROFILE,
    probe: ProbeRegisterReadout {
        who_am_i_register: REG_WHO_AM_I,
        revision_register: Some(REG_COM_CFG),
        matches: PROBE_MATCHES,
        attempts: 3,
        retry_delay_ms: PROBE_RETRY_DELAY_MS,
    },
    sample_readout: SampleRegisterReadout {
        data_start_register: REG_ACC_XH,
        byte_order: SampleByteOrder::BigEndian,
        status: Some(DataReadyStatus {
            register: REG_DATA_STAT,
            mask: 0x03,
            condition: DataReadyCondition::AnySet,
        }),
        poll_attempts: 0,
        poll_delay_ms: 0,
        read_on_timeout: false,
    },
};

pub struct Hxy42688Driver;

#[async_trait(?Send)]
impl ImuDriver for Hxy42688Driver {
    fn info(&self) -> &'static DriverInfo {
        &INFO
    }

    async fn configure(
        &self,
        bus: &mut dyn ImuBus,
        target: ImuTargetId,
        config: &ImuSampleConfig,
    ) -> Result<(), SmartImuError> {
        crate::ensure_sample_config_supported(&INFO.chip_profile.sample_config_options, config)?;

        // Enable accel/gyro before touching their range and filter registers.
        bus.write_reg(target, REG_PWR_CTRL, 0x0E)?;
        delay_ms(SENSOR_POWER_UP_DELAY_MS).await;

        // Program accel ODR/filter preset, then apply the requested full-scale range.
        bus.write_reg(target, REG_ACC_CONF, 0xA8)?;
        bus.write_reg(
            target,
            REG_ACC_RANGE,
            match config.accel_range {
                RangeG(4) => 0x01,
                RangeG(8) => 0x02,
                RangeG(16) => 0x03,
                _ => {
                    return Err(SmartImuError::UnsupportedConfig(
                        crate::UnsupportedConfigReason::AccelRange,
                    ));
                }
            },
        )?;

        // Program gyro ODR/filter preset, then apply the requested full-scale range.
        bus.write_reg(target, REG_GYR_CONF, 0xA9)?;
        bus.write_reg(
            target,
            REG_GYR_RANGE,
            match config.gyro_range {
                RangeDps(2000) => 0x00,
                RangeDps(1000) => 0x01,
                RangeDps(500) => 0x02,
                RangeDps(250) => 0x03,
                _ => {
                    return Err(SmartImuError::UnsupportedConfig(
                        crate::UnsupportedConfigReason::GyroRange,
                    ));
                }
            },
        )?;

        // Give the output pipeline a short settle window before the first sample read.
        delay_ms(CONFIG_SETTLE_DELAY_MS).await;
        Ok(())
    }
}
