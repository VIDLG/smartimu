use crate::{
    BusInfo, DeviceEvent, DeviceHeader, DeviceMessage, DeviceResponse, ErrorEvent, HeartbeatEvent,
    ImuId, ImuNodeInfo, ImuNodeInfoPayload, ImuNodeInfoResponse, InventoryPayload,
    InventoryResponse, MAX_MESSAGE_LEN, PROTOCOL_VERSION, PongPayload, PongResponse,
    ProbeDetectedEvent, ProbeDetectedPayload, ProtocolError, RawImuSample, RawSampleEvent,
    RawSamplePayload, ResponseResult, SmartImuError, StartSamplingPayload, StartSamplingResponse,
    StopSamplingPayload, StopSamplingResponse, SystemInfo, WireFormat,
};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

pub struct SessionRuntime {
    pub system_id: u16,
    pub session_id: u32,
    pub format: WireFormat,
    seq: u32,
}

impl SessionRuntime {
    pub const fn new(system_id: u16, session_id: u32, format: WireFormat) -> Self {
        Self {
            system_id,
            session_id,
            format,
            seq: 0,
        }
    }

    pub fn header(&mut self, timestamp_us: u64) -> DeviceHeader {
        let header = DeviceHeader {
            protocol_version: PROTOCOL_VERSION,
            format: self.format,
            system_id: self.system_id,
            session_id: self.session_id,
            seq: self.seq,
            timestamp_us,
        };
        self.seq = self.seq.wrapping_add(1);
        header
    }

    pub fn pong(&mut self, timestamp_us: u64, message: &str) -> DeviceMessage {
        DeviceMessage::Response(DeviceResponse::Pong(PongResponse {
            header: self.header(timestamp_us),
            result: ResponseResult::Ok(PongPayload {
                message: bounded_string(message, crate::MAX_MESSAGE_LEN),
            }),
        }))
    }

    pub fn inventory_response(
        &mut self,
        timestamp_us: u64,
        system_label: &str,
        buses: Vec<BusInfo>,
        imus: Vec<ImuNodeInfo>,
    ) -> DeviceMessage {
        DeviceMessage::Response(DeviceResponse::Inventory(InventoryResponse {
            header: self.header(timestamp_us),
            result: ResponseResult::Ok(InventoryPayload {
                system: SystemInfo {
                    system_id: self.system_id,
                    label: bounded_string(system_label, crate::MAX_LABEL_LEN),
                },
                buses,
                imus,
            }),
        }))
    }

    pub fn imu_node_info_response(
        &mut self,
        timestamp_us: u64,
        imu_id: ImuId,
        info: ImuNodeInfo,
    ) -> DeviceMessage {
        DeviceMessage::Response(DeviceResponse::ImuNodeInfo(ImuNodeInfoResponse {
            header: self.header(timestamp_us),
            result: ResponseResult::Ok(ImuNodeInfoPayload { imu_id, info }),
        }))
    }

    pub fn imu_node_info_not_found_response(
        &mut self,
        timestamp_us: u64,
        _imu_id: ImuId,
    ) -> DeviceMessage {
        DeviceMessage::Response(DeviceResponse::ImuNodeInfo(ImuNodeInfoResponse {
            header: self.header(timestamp_us),
            result: ResponseResult::Err(ProtocolError {
                imu_id: Some(_imu_id),
                error: SmartImuError::ImuNotFound,
                message: bounded_string("IMU node info not found", MAX_MESSAGE_LEN),
            }),
        }))
    }

    pub fn start_sampling_response(
        &mut self,
        timestamp_us: u64,
        imu_ids: Vec<ImuId>,
    ) -> DeviceMessage {
        DeviceMessage::Response(DeviceResponse::StartSampling(StartSamplingResponse {
            header: self.header(timestamp_us),
            result: ResponseResult::Ok(StartSamplingPayload { imu_ids }),
        }))
    }

    pub fn stop_sampling_response(
        &mut self,
        timestamp_us: u64,
        imu_ids: Vec<ImuId>,
    ) -> DeviceMessage {
        DeviceMessage::Response(DeviceResponse::StopSampling(StopSamplingResponse {
            header: self.header(timestamp_us),
            result: ResponseResult::Ok(StopSamplingPayload { imu_ids }),
        }))
    }

    pub fn probe_detected(
        &mut self,
        timestamp_us: u64,
        imu_id: ImuId,
        driver_id: &str,
        spi_profile: crate::SpiProfile,
        chip_info: crate::DetectedChipInfo,
    ) -> DeviceMessage {
        DeviceMessage::Event(DeviceEvent::ProbeDetected(ProbeDetectedEvent {
            header: self.header(timestamp_us),
            payload: ProbeDetectedPayload {
                imu_id,
                driver_id: bounded_string(driver_id, crate::MAX_LABEL_LEN),
                spi_profile,
                chip_info,
            },
        }))
    }

    pub fn raw_sample(
        &mut self,
        timestamp_us: u64,
        imu_id: ImuId,
        sample_index: u32,
        sample_timestamp_us: u64,
        sample: RawImuSample,
    ) -> DeviceMessage {
        DeviceMessage::Event(DeviceEvent::RawSample(RawSampleEvent {
            header: self.header(timestamp_us),
            payload: RawSamplePayload {
                imu_id,
                sample_index,
                timestamp_us: sample_timestamp_us,
                sample,
            },
        }))
    }

    pub fn error(
        &mut self,
        timestamp_us: u64,
        imu_id: Option<ImuId>,
        error: SmartImuError,
        message: &str,
    ) -> DeviceMessage {
        DeviceMessage::Event(DeviceEvent::Error(ErrorEvent {
            header: self.header(timestamp_us),
            payload: ProtocolError {
                imu_id,
                error,
                message: bounded_string(message, MAX_MESSAGE_LEN),
            },
        }))
    }

    pub fn heartbeat(&mut self, timestamp_us: u64, active_imu_ids: Vec<ImuId>) -> DeviceMessage {
        DeviceMessage::Event(DeviceEvent::Heartbeat(HeartbeatEvent {
            header: self.header(timestamp_us),
            payload: crate::HeartbeatPayload { active_imu_ids },
        }))
    }
}

pub fn bounded_string(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

pub fn protocol_string(value: &str) -> String {
    value.to_string()
}
