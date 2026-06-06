use crate::bus::{ImuBus, ImuTargetId};
use crate::delay_ms;
use crate::error::{SmartImuError, UnsupportedConfigReason};
use crate::protocol::SampleReadoutRequest;
use crate::sample::{RawImu6, RawImuSample};
use crate::types::{
    DetectedChipInfo, ImuChipProfile, ImuId, ImuIdentity, ImuSampleConfig, SampleConfigCapability,
};
use alloc::boxed::Box;
use async_trait::async_trait;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImuTargetInfo {
    pub id: ImuId,
    pub target: ImuTargetId,
}

pub struct DriverInfo {
    pub name: &'static str,
    pub driver: &'static dyn ImuDriver,
    pub chip_profile: &'static ImuChipProfile,
    pub probe: ProbeRegisterReadout,
    pub sample_readout: SampleRegisterReadout,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeRegisterMatch {
    WhoAmI(u8),
    WhoAmIAndRevision { who_am_i: u8, revision: u8 },
    WhoAmIAndNotRevision { who_am_i: u8, revision: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProbeRegisterReadout {
    pub who_am_i_register: u8,
    pub revision_register: Option<u8>,
    pub matches: &'static [ProbeRegisterMatch],
    pub attempts: u8,
    pub retry_delay_ms: u64,
}

impl ProbeRegisterMatch {
    fn matches(self, who_am_i: u8, revision: Option<u8>) -> bool {
        match self {
            Self::WhoAmI(expected) => who_am_i == expected,
            Self::WhoAmIAndRevision {
                who_am_i: expected_id,
                revision: expected_revision,
            } => who_am_i == expected_id && revision == Some(expected_revision),
            Self::WhoAmIAndNotRevision {
                who_am_i: expected_id,
                revision: excluded_revision,
            } => who_am_i == expected_id && revision != Some(excluded_revision),
        }
    }
}

impl ProbeRegisterReadout {
    pub async fn read(
        self,
        bus: &mut dyn ImuBus,
        target: ImuTargetId,
        chip_profile: &'static ImuChipProfile,
    ) -> Result<Option<DetectedChipInfo>, SmartImuError> {
        for _ in 0..self.attempts {
            let who_am_i = bus.read_reg(target, self.who_am_i_register, crate::Turnaround(0))?;
            let revision = match self.revision_register {
                Some(register) => Some(bus.read_reg(target, register, crate::Turnaround(0))?),
                None => None,
            };

            if self
                .matches
                .iter()
                .any(|probe_match| probe_match.matches(who_am_i, revision))
            {
                return Ok(Some(DetectedChipInfo {
                    chip_profile: chip_profile.clone(),
                    identity: ImuIdentity { who_am_i, revision },
                }));
            }

            delay_ms(self.retry_delay_ms).await;
        }

        Ok(None)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleByteOrder {
    BigEndian,
    LittleEndian,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataReadyCondition {
    AnySet,
    Equals(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DataReadyStatus {
    pub register: u8,
    pub mask: u8,
    pub condition: DataReadyCondition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SampleRegisterReadout {
    pub data_start_register: u8,
    pub byte_order: SampleByteOrder,
    pub status: Option<DataReadyStatus>,
    pub poll_attempts: u8,
    pub poll_delay_ms: u64,
    pub read_on_timeout: bool,
}

impl DataReadyStatus {
    fn is_ready(self, status: u8) -> bool {
        let masked = status & self.mask;
        match self.condition {
            DataReadyCondition::AnySet => masked != 0,
            DataReadyCondition::Equals(expected) => masked == expected,
        }
    }
}

impl SampleRegisterReadout {
    pub async fn read(
        self,
        bus: &mut dyn ImuBus,
        target: ImuTargetId,
    ) -> Result<RawImuSample, SmartImuError> {
        if let Some(status) = self.status {
            if self.poll_attempts == 0 {
                let value = bus.read_reg(target, status.register, crate::Turnaround(0))?;
                if !status.is_ready(value) {
                    return Err(SmartImuError::DataNotReady);
                }
            } else {
                for _ in 0..self.poll_attempts {
                    let value = bus.read_reg(target, status.register, crate::Turnaround(0))?;
                    if status.is_ready(value) {
                        return self.read_imu6(bus, target).map(Into::into);
                    }
                    delay_ms(self.poll_delay_ms).await;
                }

                if !self.read_on_timeout {
                    return Err(SmartImuError::DataNotReady);
                }
            }
        }

        self.read_imu6(bus, target).map(Into::into)
    }

    fn read_imu6(
        self,
        bus: &mut dyn ImuBus,
        target: ImuTargetId,
    ) -> Result<RawImu6, SmartImuError> {
        let mut buf = [0u8; 12];
        bus.read_regs(
            target,
            self.data_start_register,
            crate::Turnaround(0),
            &mut buf,
        )?;
        Ok(RawImu6 {
            accel: [
                self.read_i16(buf[0], buf[1]),
                self.read_i16(buf[2], buf[3]),
                self.read_i16(buf[4], buf[5]),
            ],
            gyro: [
                self.read_i16(buf[6], buf[7]),
                self.read_i16(buf[8], buf[9]),
                self.read_i16(buf[10], buf[11]),
            ],
        })
    }

    fn read_i16(self, first: u8, second: u8) -> i16 {
        match self.byte_order {
            SampleByteOrder::BigEndian => i16::from_be_bytes([first, second]),
            SampleByteOrder::LittleEndian => i16::from_le_bytes([first, second]),
        }
    }
}

#[async_trait(?Send)]
pub trait ImuDriver: Sync {
    fn info(&self) -> &'static DriverInfo;

    async fn probe(
        &self,
        bus: &mut dyn ImuBus,
        target: ImuTargetId,
    ) -> Result<Option<DetectedChipInfo>, SmartImuError> {
        let info = self.info();
        info.probe.read(bus, target, info.chip_profile).await
    }
    async fn reset(
        &self,
        _bus: &mut dyn ImuBus,
        _target: ImuTargetId,
    ) -> Result<(), SmartImuError> {
        Ok(())
    }
    async fn configure(
        &self,
        bus: &mut dyn ImuBus,
        target: ImuTargetId,
        config: &ImuSampleConfig,
    ) -> Result<(), SmartImuError>;
    async fn read_sample(
        &self,
        bus: &mut dyn ImuBus,
        target: ImuTargetId,
        request: SampleReadoutRequest,
    ) -> Result<RawImuSample, SmartImuError> {
        let info = self.info();
        ensure_sample_readout_allowed(
            info.chip_profile.sensor_timestamp,
            info.chip_profile.temperature_scale.as_ref(),
            request,
        )?;
        info.sample_readout.read(bus, target).await
    }
}

pub fn ensure_sample_config_allowed(
    sample_config_capability: &SampleConfigCapability,
    config: &ImuSampleConfig,
) -> Result<(), SmartImuError> {
    if sample_config_capability.contains(config) {
        Ok(())
    } else {
        Err(SmartImuError::UnsupportedConfig(
            UnsupportedConfigReason::SampleConfig,
        ))
    }
}

pub fn ensure_sample_readout_allowed(
    sensor_timestamp: bool,
    temperature_scale: Option<&crate::TemperatureScale>,
    request: SampleReadoutRequest,
) -> Result<(), SmartImuError> {
    if request.temperature && temperature_scale.is_none() {
        return Err(SmartImuError::UnsupportedConfig(
            UnsupportedConfigReason::TemperatureReadout,
        ));
    }
    if request.sensor_timestamp && !sensor_timestamp {
        return Err(SmartImuError::UnsupportedConfig(
            UnsupportedConfigReason::SensorTimestampReadout,
        ));
    }
    Ok(())
}
