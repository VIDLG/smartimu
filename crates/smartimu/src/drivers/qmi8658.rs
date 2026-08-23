use crate::{
    DataReadyCondition, DataReadyStatus, DriverInfo, ImuBus, ImuChipModel, ImuChipProfile,
    ImuDriver, ImuSampleConfig, ImuTargetId, ProbeRegisterMatch, ProbeRegisterReadout, RangeDps,
    RangeG, SampleByteOrder, SampleRateHz, SampleRegisterReadout, SmartImuError, delay_ms,
};
use alloc::{borrow::Cow, boxed::Box};
use async_trait::async_trait;

const CHIP_ID: u8 = 0x05;
const CHIP_ID_ALT: u8 = 0x3E;
const PROBE_RETRY_DELAY_MS: u64 = 5;
const SOFT_RESET_DELAY_MS: u64 = 20;
const CONFIG_SETTLE_DELAY_MS: u64 = 50;

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

const ACCEL_RANGES: &[RangeG] = &[RangeG(2)];
const GYRO_RANGES: &[RangeDps] = &[RangeDps(2048)];
const SAMPLE_RATES: &[SampleRateHz] = &[SampleRateHz(100)];
const PROBE_MATCHES: &[ProbeRegisterMatch] = &[
    ProbeRegisterMatch::WhoAmI(CHIP_ID),
    ProbeRegisterMatch::WhoAmIAndRevision {
        who_am_i: CHIP_ID_ALT,
        revision: CHIP_ID_ALT,
    },
];

pub static CHIP_PROFILE: ImuChipProfile = ImuChipProfile {
    model: ImuChipModel::Qmi8658A,
    sample_config_capability: crate::SampleConfigCapability::Independent {
        accel_ranges: Cow::Borrowed(ACCEL_RANGES),
        gyro_ranges: Cow::Borrowed(GYRO_RANGES),
        sample_rates: Cow::Borrowed(SAMPLE_RATES),
    },
    sensor_timestamp: false,
    temperature_scale: None,
};

pub static DRIVER: Qmi8658Driver = Qmi8658Driver;
pub static INFO: crate::DriverInfo = crate::DriverInfo {
    name: "QMI8658A",
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

pub struct Qmi8658Driver;

#[async_trait(?Send)]
impl ImuDriver for Qmi8658Driver {
    fn info(&self) -> &'static DriverInfo {
        &INFO
    }

    async fn reset(&self, bus: &mut dyn ImuBus, target: ImuTargetId) -> Result<(), SmartImuError> {
        // Soft reset clears prior board state before applying our fixed startup preset.
        bus.write_reg(target, REG_RESET, 0xB0)?;
        delay_ms(SOFT_RESET_DELAY_MS).await;
        Ok(())
    }

    async fn configure(
        &self,
        bus: &mut dyn ImuBus,
        target: ImuTargetId,
        config: &ImuSampleConfig,
    ) -> Result<(), SmartImuError> {
        crate::ensure_sample_config_allowed(&INFO.chip_profile.sample_config_capability, config)?;

        // Apply the board-validated startup preset for output rate, ranges, and data path.
        bus.write_reg(target, REG_CTRL1, 0x20)?;
        bus.write_reg(target, REG_CTRL2, 0x06)?;
        bus.write_reg(target, REG_CTRL3, 0x76)?;
        bus.write_reg(target, REG_CTRL5, 0x00)?;
        bus.write_reg(target, REG_CTRL7, 0x03)?;

        // QMI has been touchier after enable/reset, so keep this conservative for now.
        delay_ms(CONFIG_SETTLE_DELAY_MS).await;
        Ok(())
    }
}
