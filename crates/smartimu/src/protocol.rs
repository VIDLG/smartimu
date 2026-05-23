use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::bus::SpiProfile;
use crate::error::SmartImuError;
use crate::sample::{RawImuSample, SampleReadoutRequest};
use crate::types::{BusInfo, ImuChip, ImuId, ImuInfo, Quaternion, SystemInfo};

pub const PROTOCOL_VERSION: u8 = 1;
pub const MAX_IMUS_PER_SYSTEM: usize = 16;
pub const MAX_BUSES_PER_SYSTEM: usize = 8;
pub const MAX_LABEL_LEN: usize = 32;
pub const MAX_MESSAGE_LEN: usize = 96;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceHeader {
    pub protocol_version: u8,
    pub format: WireFormat,
    /// Identifies which physical board is sending this frame.
    /// Allows a host to distinguish data from multiple boards.
    pub system_id: u16,
    /// Distinguishes different run sessions on the same board (increments on reboot).
    pub session_id: u32,
    /// Monotonically increasing sequence number for ordering and loss detection.
    pub seq: u32,
    /// Device-side timestamp captured when this frame is emitted.
    pub emit_timestamp_us: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostHeader {
    pub protocol_version: u8,
    pub seq: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PingRequestFrame {
    pub header: HostHeader,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetInventoryRequestFrame {
    pub header: HostHeader,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetImuInfoRequestFrame {
    pub header: HostHeader,
    pub imu_id: ImuId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartSamplingRequestFrame {
    pub header: HostHeader,
    pub imu_id: Option<ImuId>,
    pub sample_request: SampleReadoutRequest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopSamplingRequestFrame {
    pub header: HostHeader,
    pub imu_id: Option<ImuId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PingFrame {
    pub header: DeviceHeader,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InventoryFrame {
    pub header: DeviceHeader,
    pub system: SystemInfo,
    pub buses: Vec<BusInfo>,
    pub imus: Vec<ImuInfo>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImuInfoFrame {
    pub header: DeviceHeader,
    pub imu_id: ImuId,
    pub info: Option<ImuInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeResultFrame {
    pub header: DeviceHeader,
    pub imu_id: ImuId,
    pub probe_label: String,
    pub result: ProbeResult,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProbeResult {
    Detected {
        driver_id: String,
        chip: ImuChip,
        profile: SpiProfile,
    },
    NotDetected,
    Failed {
        error: SmartImuError,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SampleFrame {
    pub header: DeviceHeader,
    pub imu_id: ImuId,
    pub imu_chip: ImuChip,
    pub sample_index: u32,
    pub sample_timestamp_us: u64,
    pub sample: RawImuSample,
    pub status_bits: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrientationFrame {
    pub header: DeviceHeader,
    pub imu_id: ImuId,
    pub imu_chip: ImuChip,
    pub sample_index: u32,
    pub sample_timestamp_us: u64,
    pub quaternion: Quaternion,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorFrame {
    pub header: DeviceHeader,
    pub imu_id: Option<ImuId>,
    pub error: SmartImuError,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeartbeatFrame {
    pub header: DeviceHeader,
    pub active_imus: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum HostFrame {
    Ping(PingRequestFrame),
    GetInventory(GetInventoryRequestFrame),
    GetImuInfo(GetImuInfoRequestFrame),
    StartSampling(StartSamplingRequestFrame),
    StopSampling(StopSamplingRequestFrame),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DeviceFrame {
    Ping(PingFrame),
    Inventory(InventoryFrame),
    ImuInfo(ImuInfoFrame),
    ProbeResult(ProbeResultFrame),
    Sample(SampleFrame),
    Orientation(OrientationFrame),
    Error(ErrorFrame),
    Heartbeat(HeartbeatFrame),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum WireFrame {
    Host(HostFrame),
    Device(DeviceFrame),
}

pub fn encode_binary<const N: usize>(frame: &WireFrame) -> BinaryCodecResult<Vec<u8>> {
    let encoded = postcard::to_allocvec(frame).map_err(|_| BinaryCodecError::Postcard)?;
    if encoded.len() > N {
        return Err(BinaryCodecError::BufferTooSmall);
    }
    Ok(encoded)
}

pub fn decode_binary(bytes: &[u8]) -> Result<WireFrame, postcard::Error> {
    postcard::from_bytes(bytes)
}

pub fn encode_binary_packet<const N: usize>(frame: &WireFrame) -> BinaryCodecResult<Vec<u8>> {
    let mut raw = postcard::to_allocvec(frame).map_err(|_| BinaryCodecError::Postcard)?;
    if raw.len() + 4 > N {
        return Err(BinaryCodecError::BufferTooSmall);
    }
    let crc = crc32fast::hash(raw.as_slice()).to_le_bytes();
    for byte in crc {
        raw.push(byte);
    }

    let encoded_len = cobs::max_encoding_length(raw.len());
    if encoded_len + 1 > N {
        return Err(BinaryCodecError::BufferTooSmall);
    }

    let mut scratch = [0u8; N];
    let used = cobs::encode(raw.as_slice(), &mut scratch);
    let mut framed = Vec::new();
    for byte in &scratch[..used] {
        framed.push(*byte);
    }
    framed.push(0);
    Ok(framed)
}

pub fn decode_binary_packet<const N: usize>(packet: &[u8]) -> BinaryCodecResult<WireFrame> {
    if packet.is_empty() {
        return Err(BinaryCodecError::Truncated);
    }

    let encoded = if packet.last() == Some(&0) {
        &packet[..packet.len() - 1]
    } else {
        packet
    };

    if encoded.is_empty() {
        return Err(BinaryCodecError::Truncated);
    }

    let mut decoded = [0u8; N];
    let used = cobs::decode(encoded, &mut decoded).map_err(|_| BinaryCodecError::CobsDecode)?;
    if used < 4 {
        return Err(BinaryCodecError::Truncated);
    }

    let payload_len = used - 4;
    let payload = &decoded[..payload_len];
    let crc_bytes: [u8; 4] = decoded[payload_len..used]
        .try_into()
        .map_err(|_| BinaryCodecError::Truncated)?;
    let expected_crc = u32::from_le_bytes(crc_bytes);
    let actual_crc = crc32fast::hash(payload);
    if expected_crc != actual_crc {
        return Err(BinaryCodecError::CrcMismatch);
    }

    postcard::from_bytes(payload).map_err(|_| BinaryCodecError::Postcard)
}

#[cfg(feature = "json")]
pub fn encode_json<const N: usize>(
    frame: &WireFrame,
) -> Result<String, serde_json_core::ser::Error> {
    let mut output = [0u8; N];
    let written = serde_json_core::to_slice(frame, &mut output)?;
    core::str::from_utf8(&output[..written])
        .map(String::from)
        .map_err(|_| serde_json_core::ser::Error::BufferFull)
}

#[cfg(feature = "std-json")]
pub fn decode_json(line: &str) -> Result<WireFrame, serde_json::Error> {
    serde_json::from_str(line)
}
