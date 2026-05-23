#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriverResourceKey {
    Bmi270ConfigBlob,
}

pub trait DriverResources {
    fn bytes(&self, key: DriverResourceKey) -> Option<&[u8]>;
}
