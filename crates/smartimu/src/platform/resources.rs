use crate::{DriverResourceKey, DriverResources};

mod generated {
    include!(concat!(env!("OUT_DIR"), "/bmi270_config.rs"));
}

pub struct EspDriverResources;

impl DriverResources for EspDriverResources {
    fn bytes(&self, key: DriverResourceKey) -> Option<&[u8]> {
        match key {
            DriverResourceKey::Bmi270ConfigBlob => Some(generated::BMI270_CONFIG),
        }
    }
}
