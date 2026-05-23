pub mod hxy42688;
pub mod icm42688;
pub mod lsm6;
pub mod qmi8658;

use alloc::borrow::Cow;

use crate::{
    ImuChip, ImuChipProfile, RangeDps, RangeG, SampleConfigOptions, SampleRateHz,
    SampleReadoutSupport,
};

pub(super) const fn six_axis_chip_profile(
    chip: ImuChip,
    accel_ranges: &'static [RangeG],
    gyro_ranges: &'static [RangeDps],
    sample_rates: &'static [SampleRateHz],
) -> ImuChipProfile {
    ImuChipProfile {
        chip,
        sample_config_options: SampleConfigOptions::Independent {
            accel_ranges: Cow::Borrowed(accel_ranges),
            gyro_ranges: Cow::Borrowed(gyro_ranges),
            sample_rates: Cow::Borrowed(sample_rates),
        },
        sample_readout_support: SampleReadoutSupport {
            temperature: false,
            sensor_timestamp: false,
        },
        temperature_config: None,
    }
}
