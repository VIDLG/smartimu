use crate::bus::{ImuBus, ImuTargetId, SpiProfile};
use crate::driver::ImuDriver;
use crate::drivers::CandidateDriver;
use crate::error::ImuError;

pub fn probe_first_matching(
    bus: &mut dyn ImuBus<Profile = SpiProfile>,
    target: ImuTargetId,
    candidates: &[CandidateDriver],
) -> Result<Option<(&'static dyn ImuDriver, SpiProfile)>, ImuError> {
    for candidate in candidates {
        for profile in candidate.profiles {
            bus.apply_profile(target, *profile)?;
            bus.delay_ms(1);

            match candidate.descriptor.driver.probe(bus, target) {
                Ok(true) => return Ok(Some((candidate.descriptor.driver, *profile))),
                Ok(false) => continue,
                Err(ImuError::CommunicationError) => continue,
                Err(error) => return Err(error),
            }
        }
    }

    Ok(None)
}
