use crate::bus::{ImuBus, ImuTargetId, SpiProfile};
use crate::delay_ms;
use crate::device::DetectedImuDevice;
use crate::driver::{DriverInfo, ImuDriver};
use crate::error::SmartImuError;
use crate::types::ProbeInfo;

#[derive(Clone, Copy)]
pub struct CandidateDriver {
    pub info: &'static DriverInfo,
    pub profiles: &'static [SpiProfile],
}

#[derive(Clone, Copy)]
pub enum ProbePlan<'a> {
    Auto {
        candidates: &'a [CandidateDriver],
    },
    Manual {
        driver: &'static dyn ImuDriver,
        profiles: &'a [SpiProfile],
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProbeMatch {
    pub driver: &'static dyn ImuDriver,
    pub profile: SpiProfile,
    pub info: ProbeInfo,
}

impl ProbeMatch {
    pub fn into_detected_device(self, target: ImuTargetId) -> DetectedImuDevice {
        DetectedImuDevice::new(self.driver, target, self.profile, self.info)
    }
}

pub async fn probe(
    bus: &mut dyn ImuBus,
    target: ImuTargetId,
    plan: ProbePlan<'_>,
) -> Result<Option<ProbeMatch>, SmartImuError> {
    match plan {
        ProbePlan::Auto { candidates } => probe_first_matching(bus, target, candidates).await,
        ProbePlan::Manual { driver, profiles } => probe_driver(bus, target, driver, profiles).await,
    }
}

pub async fn probe_first_matching(
    bus: &mut dyn ImuBus,
    target: ImuTargetId,
    candidates: &[CandidateDriver],
) -> Result<Option<ProbeMatch>, SmartImuError> {
    for candidate in candidates {
        if let Some(probe_match) =
            probe_driver(bus, target, candidate.info.driver, candidate.profiles).await?
        {
            return Ok(Some(probe_match));
        }
    }

    Ok(None)
}

pub async fn probe_driver(
    bus: &mut dyn ImuBus,
    target: ImuTargetId,
    driver: &'static dyn ImuDriver,
    profiles: &[SpiProfile],
) -> Result<Option<ProbeMatch>, SmartImuError> {
    for profile in profiles {
        bus.apply_profile(target, *profile)?;
        delay_ms(1).await;

        match driver.probe(bus, target).await {
            Ok(Some(info)) => {
                return Ok(Some(ProbeMatch {
                    driver,
                    profile: *profile,
                    info,
                }));
            }
            Ok(None) => continue,
            Err(SmartImuError::CommunicationError) => continue,
            Err(error) => return Err(error),
        }
    }

    Ok(None)
}
