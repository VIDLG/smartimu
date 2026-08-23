use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::bus::SpiProfile;
use crate::error::SmartImuError;
use crate::sample::RawImuSample;
use crate::types::{
    BusInfo, DetectedChipInfo, DriverId, ImuDeviceInfo, ImuId, LowPowerSeverity, MessageSeq,
    PowerStatus, Quaternion, SampleIndex, SessionId, SystemId, SystemInfo, TimestampUs,
};

pub const PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion { major: 1, minor: 0 };
pub const MAX_IMUS_PER_SYSTEM: usize = 16;
pub const MAX_BUSES_PER_SYSTEM: usize = 8;
pub const MAX_BINARY_PACKET_LEN: usize = 1470;

pub type BinaryCodecResult<T> = Result<T, BinaryCodecError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Error)]
pub enum BinaryCodecError {
    #[error("binary codec buffer too small")]
    BufferTooSmall,
    #[error("postcard serialization error")]
    Postcard,
    #[error("COBS decode error")]
    CobsDecode,
    #[error("CRC mismatch")]
    CrcMismatch,
    #[error("truncated binary packet")]
    Truncated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireFormat {
    Binary,
    Json,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolVersion {
    pub major: u8,
    pub minor: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceHeader {
    pub protocol_version: ProtocolVersion,
    /// Identifies which physical board is sending this message.
    /// Allows a host to distinguish data from multiple boards.
    pub system_id: SystemId,
    /// Distinguishes different run sessions on the same board (increments on reboot).
    pub session_id: SessionId,
    /// Monotonically increasing sequence number for ordering and loss detection.
    pub seq: MessageSeq,
    /// Device-side timestamp captured when this protocol message is emitted.
    /// Raw samples and orientations carry their own data timestamps in the payload.
    pub timestamp_us: TimestampUs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostHeader {
    pub protocol_version: ProtocolVersion,
    /// Host-side request sequence number. Host requests intentionally do not
    /// carry timestamps because host and device clocks are different domains.
    pub seq: MessageSeq,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PingRequest {
    pub header: HostHeader,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseResult<T> {
    Ok(T),
    Err(ProtocolError),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolError {
    pub imu_id: Option<ImuId>,
    pub code: ProtocolErrorCode,
    pub details: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtocolErrorCode {
    CommunicationError,
    ChipNotFound,
    ImuNotFound,
    ConfigError,
    DataNotReady,
    MissingResource,
    UnsupportedConfig,
    InvalidTarget,
    Internal,
}

impl From<SmartImuError> for ProtocolErrorCode {
    fn from(error: SmartImuError) -> Self {
        match error {
            SmartImuError::CommunicationError => Self::CommunicationError,
            SmartImuError::ChipNotFound => Self::ChipNotFound,
            SmartImuError::ImuNotFound => Self::ImuNotFound,
            SmartImuError::ConfigError => Self::ConfigError,
            SmartImuError::DataNotReady => Self::DataNotReady,
            SmartImuError::MissingResource => Self::MissingResource,
            SmartImuError::UnsupportedConfig(_) => Self::UnsupportedConfig,
            SmartImuError::InvalidTarget => Self::InvalidTarget,
        }
    }
}

impl From<SmartImuError> for ProtocolError {
    fn from(error: SmartImuError) -> Self {
        Self {
            imu_id: None,
            code: ProtocolErrorCode::from(error),
            details: alloc::format!("{error}"),
        }
    }
}

impl ProtocolError {
    pub fn with_imu_id(mut self, imu_id: Option<ImuId>) -> Self {
        self.imu_id = imu_id;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Response<T> {
    pub header: DeviceHeader,
    pub result: ResponseResult<T>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event<T> {
    pub header: DeviceHeader,
    pub payload: T,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PongPayload {
    pub message: String,
}

pub type PongResponse = Response<PongPayload>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetInventoryRequest {
    pub header: HostHeader,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InventoryPayload {
    pub system: SystemInfo,
    pub buses: Vec<BusInfo>,
    pub imu_devices: Vec<ImuDeviceInfo>,
}

pub type InventoryResponse = Response<InventoryPayload>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetImuDeviceInfoRequest {
    pub header: HostHeader,
    pub imu_id: ImuId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImuDeviceInfoPayload {
    pub imu_id: ImuId,
    pub info: ImuDeviceInfo,
}

pub type ImuDeviceInfoResponse = Response<ImuDeviceInfoPayload>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetPowerStatusRequest {
    pub header: HostHeader,
}

pub type PowerStatusResponse = Response<PowerStatus>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImuSelection {
    All,
    One(ImuId),
    Many(Vec<ImuId>),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SampleReadoutRequest {
    pub temperature: bool,
    pub sensor_timestamp: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartSamplingRequest {
    pub header: HostHeader,
    pub selection: ImuSelection,
    pub sample_request: SampleReadoutRequest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartSamplingPayload {
    pub imu_ids: Vec<ImuId>,
}

pub type StartSamplingResponse = Response<StartSamplingPayload>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopSamplingRequest {
    pub header: HostHeader,
    pub selection: ImuSelection,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopSamplingPayload {
    pub imu_ids: Vec<ImuId>,
}

pub type StopSamplingResponse = Response<StopSamplingPayload>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProbeDetectedPayload {
    pub imu_id: ImuId,
    pub driver_id: DriverId,
    pub spi_profile: SpiProfile,
    pub chip_info: DetectedChipInfo,
}

pub type ProbeDetectedEvent = Event<ProbeDetectedPayload>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawSamplePayload {
    pub imu_id: ImuId,
    pub sample_index: SampleIndex,
    /// Device-side timestamp for when this raw sample was captured/read.
    pub timestamp_us: TimestampUs,
    pub sample: RawImuSample,
}

pub type RawSampleEvent = Event<RawSamplePayload>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrientationPayload {
    pub imu_id: ImuId,
    pub sample_index: SampleIndex,
    /// Device-side timestamp for the sample time represented by this orientation.
    pub timestamp_us: TimestampUs,
    pub quaternion: Quaternion,
}

pub type OrientationEvent = Event<OrientationPayload>;

pub type ErrorEvent = Event<ProtocolError>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerEventPayload {
    Status(PowerStatus),
    LowPower {
        status: PowerStatus,
        severity: LowPowerSeverity,
    },
}

pub type PowerEvent = Event<PowerEventPayload>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeartbeatPayload {
    pub active_imu_ids: Vec<ImuId>,
}

pub type HeartbeatEvent = Event<HeartbeatPayload>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum HostRequest {
    Ping(PingRequest),
    GetInventory(GetInventoryRequest),
    GetImuDeviceInfo(GetImuDeviceInfoRequest),
    GetPowerStatus(GetPowerStatusRequest),
    StartSampling(StartSamplingRequest),
    StopSampling(StopSamplingRequest),
}

/// Response to a host request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DeviceResponse {
    Pong(PongResponse),
    Inventory(InventoryResponse),
    ImuDeviceInfo(ImuDeviceInfoResponse),
    PowerStatus(PowerStatusResponse),
    StartSampling(StartSamplingResponse),
    StopSampling(StopSamplingResponse),
}

/// Device-originated notification that is not tied to a specific host request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DeviceEvent {
    ProbeDetected(ProbeDetectedEvent),
    RawSample(RawSampleEvent),
    Orientation(OrientationEvent),
    Error(ErrorEvent),
    Power(PowerEvent),
    Heartbeat(HeartbeatEvent),
}

/// Any message emitted by the device.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DeviceMessage {
    Response(DeviceResponse),
    Event(DeviceEvent),
}

/// Top-level message carried by JSON, binary serial, or other transports.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum WireMessage {
    HostRequest(HostRequest),
    DeviceMessage(DeviceMessage),
}

#[derive(Debug, Default)]
pub struct BinaryEncoder {
    raw: Vec<u8>,
    framed: Vec<u8>,
}

impl BinaryEncoder {
    pub fn new() -> Self {
        Self {
            raw: Vec::with_capacity(MAX_BINARY_PACKET_LEN),
            framed: Vec::with_capacity(MAX_BINARY_PACKET_LEN),
        }
    }

    pub fn encode_packet(&mut self, message: &WireMessage) -> BinaryCodecResult<&[u8]> {
        self.framed.clear();

        self.raw.clear();
        let raw = core::mem::take(&mut self.raw);
        self.raw = postcard::to_extend(message, raw).map_err(|_| BinaryCodecError::Postcard)?;
        if self.raw.len() + 4 > MAX_BINARY_PACKET_LEN {
            return Err(BinaryCodecError::BufferTooSmall);
        }

        let crc = crc32fast::hash(self.raw.as_slice()).to_le_bytes();
        for byte in crc {
            self.raw.push(byte);
        }

        let encoded_len = cobs::max_encoding_length(self.raw.len());
        if encoded_len + 1 > MAX_BINARY_PACKET_LEN {
            return Err(BinaryCodecError::BufferTooSmall);
        }

        self.framed.resize(encoded_len, 0);
        let used = cobs::encode(self.raw.as_slice(), self.framed.as_mut_slice());
        self.framed.truncate(used);
        self.framed.push(0);
        Ok(self.framed.as_slice())
    }
}

#[derive(Debug, Default)]
pub struct BinaryDecoder {
    decoded: Vec<u8>,
}

impl BinaryDecoder {
    pub fn new() -> Self {
        Self {
            decoded: Vec::with_capacity(MAX_BINARY_PACKET_LEN),
        }
    }

    pub fn decode_packet(&mut self, packet: &[u8]) -> BinaryCodecResult<WireMessage> {
        if packet.is_empty() {
            return Err(BinaryCodecError::Truncated);
        }
        if packet.len() > MAX_BINARY_PACKET_LEN {
            return Err(BinaryCodecError::BufferTooSmall);
        }

        let encoded = if packet.last() == Some(&0) {
            &packet[..packet.len() - 1]
        } else {
            packet
        };

        if encoded.is_empty() {
            return Err(BinaryCodecError::Truncated);
        }

        self.decoded.clear();
        self.decoded.resize(encoded.len(), 0);
        let used = cobs::decode(encoded, self.decoded.as_mut_slice())
            .map_err(|_| BinaryCodecError::CobsDecode)?;
        if used < 4 {
            return Err(BinaryCodecError::Truncated);
        }

        let payload_len = used - 4;
        let payload = &self.decoded[..payload_len];
        let crc_bytes: [u8; 4] = self.decoded[payload_len..used]
            .try_into()
            .map_err(|_| BinaryCodecError::Truncated)?;
        let expected_crc = u32::from_le_bytes(crc_bytes);
        let actual_crc = crc32fast::hash(payload);
        if expected_crc != actual_crc {
            return Err(BinaryCodecError::CrcMismatch);
        }

        postcard::from_bytes(payload).map_err(|_| BinaryCodecError::Postcard)
    }
}

#[cfg(feature = "json")]
pub fn encode_json<const N: usize>(
    message: &WireMessage,
) -> Result<String, serde_json_core::ser::Error> {
    let mut output = [0u8; N];
    let written = serde_json_core::to_slice(message, &mut output)?;
    core::str::from_utf8(&output[..written])
        .map(String::from)
        .map_err(|_| serde_json_core::ser::Error::BufferFull)
}

#[cfg(feature = "json")]
pub fn decode_json(line: &str) -> Result<WireMessage, serde_json_core::de::Error> {
    serde_json_core::from_str(line).map(|(message, _used)| message)
}
