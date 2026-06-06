use crate::bus::{ImuBus, ImuTargetId, SpiProfile};
use crate::driver::ImuDriver;
use crate::error::SmartImuError;
use crate::protocol::SampleReadoutRequest;
use crate::sample::RawImuSample;
use crate::types::{DetectedChipInfo, ImuNodeInfo, ImuSampleConfig};
use alloc::string::String;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Detected;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Configured {
    pub sample_config: ImuSampleConfig,
}

#[derive(Clone)]
pub struct ImuDevice<State> {
    pub driver: &'static dyn ImuDriver,
    pub target: ImuTargetId,
    pub spi_profile: SpiProfile,
    pub probe_info: DetectedChipInfo,
    pub state: State,
}

pub type DetectedImuDevice = ImuDevice<Detected>;
pub type ConfiguredImuDevice = ImuDevice<Configured>;

impl DetectedImuDevice {
    pub fn new(
        driver: &'static dyn ImuDriver,
        target: ImuTargetId,
        spi_profile: SpiProfile,
        probe_info: DetectedChipInfo,
    ) -> Self {
        Self {
            driver,
            target,
            spi_profile,
            probe_info,
            state: Detected,
        }
    }

    pub async fn reset_and_configure(
        self,
        bus: &mut dyn ImuBus,
        sample_config: ImuSampleConfig,
    ) -> Result<ConfiguredImuDevice, SmartImuError> {
        self.driver.reset(bus, self.target).await?;
        self.driver
            .configure(bus, self.target, &sample_config)
            .await?;
        Ok(ConfiguredImuDevice {
            driver: self.driver,
            target: self.target,
            spi_profile: self.spi_profile,
            probe_info: self.probe_info,
            state: Configured { sample_config },
        })
    }
}

impl ConfiguredImuDevice {
    pub fn info(
        &self,
        id: crate::ImuId,
        bus_id: crate::BusId,
        label: Option<String>,
    ) -> ImuNodeInfo {
        ImuNodeInfo {
            id,
            bus_id,
            chip_profile: self.probe_info.chip_profile.clone(),
            label,
            sample_config: self.state.sample_config,
        }
    }

    pub async fn read_sample(
        &self,
        bus: &mut dyn ImuBus,
        request: SampleReadoutRequest,
    ) -> Result<RawImuSample, SmartImuError> {
        bus.apply_profile(self.target, self.spi_profile)?;
        self.driver.read_sample(bus, self.target, request).await
    }
}
