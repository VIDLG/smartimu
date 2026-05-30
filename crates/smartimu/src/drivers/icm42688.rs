use crate::{
    DataReadyCondition, DataReadyStatus, DriverInfo, ImuBus, ImuChip, ImuChipProfile, ImuDriver,
    ImuSampleConfig, ImuTargetId, ProbeRegisterMatch, ProbeRegisterReadout, RangeDps, RangeG,
    SampleByteOrder, SampleRateHz, SampleRegisterReadout, SmartImuError, delay_ms,
};
use alloc::{borrow::Cow, boxed::Box};
use async_trait::async_trait;

const CHIP_ID: u8 = 0x05;
const REVISION_ID: u8 = 0x7C;
const PROBE_RETRY_DELAY_MS: u64 = 5;
const CONFIG_SETTLE_DELAY_MS: u64 = 20;

const REG_WHO_AM_I: u8 = 0x00;
const REG_REVISION_ID: u8 = 0x01;
const REG_CTRL1: u8 = 0x02;
const REG_CTRL2: u8 = 0x03;
const REG_CTRL3: u8 = 0x04;
const REG_CTRL5: u8 = 0x06;
const REG_CTRL7: u8 = 0x08;
const REG_STATUS0: u8 = 0x2E;
const REG_AX_L: u8 = 0x35;

const ACCEL_RANGES: &[RangeG] = &[RangeG(2)];
const GYRO_RANGES: &[RangeDps] = &[RangeDps(2048)];
const SAMPLE_RATES: &[SampleRateHz] = &[SampleRateHz(100)];
const PROBE_MATCHES: &[ProbeRegisterMatch] = &[ProbeRegisterMatch::WhoAmIAndRevision {
    who_am_i: CHIP_ID,
    revision: REVISION_ID,
}];

pub static CHIP_PROFILE: ImuChipProfile = ImuChipProfile {
    chip: ImuChip::Icm42688Pc,
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

pub static DRIVER: Icm42688Driver = Icm42688Driver;
pub static INFO: crate::DriverInfo = crate::DriverInfo {
    name: "ICM-42688-PC",
    driver: &DRIVER,
    chip_profile: &CHIP_PROFILE,
    probe: ProbeRegisterReadout {
        who_am_i_register: REG_WHO_AM_I,
        revision_register: Some(REG_REVISION_ID),
        matches: PROBE_MATCHES,
        attempts: 3,
        retry_delay_ms: PROBE_RETRY_DELAY_MS,
    },
    sample_readout: SampleRegisterReadout {
        data_start_register: REG_AX_L,
        byte_order: SampleByteOrder::LittleEndian,
        status: Some(DataReadyStatus {
            register: REG_STATUS0,
            mask: 0x03,
            condition: DataReadyCondition::Equals(0x03),
        }),
        poll_attempts: 10,
        poll_delay_ms: 1,
        read_on_timeout: true,
    },
};

pub struct Icm42688Driver;

#[async_trait(?Send)]
impl ImuDriver for Icm42688Driver {
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

        // Apply the board-validated startup preset for output rate, ranges, and data path.
        bus.write_reg(target, REG_CTRL1, 0x20)?;
        bus.write_reg(target, REG_CTRL2, 0x06)?;
        bus.write_reg(target, REG_CTRL3, 0x76)?;
        bus.write_reg(target, REG_CTRL5, 0x00)?;
        bus.write_reg(target, REG_CTRL7, 0x03)?;

        // Keep a conservative settle window before streaming samples from this preset.
        delay_ms(CONFIG_SETTLE_DELAY_MS).await;
        Ok(())
    }
}
