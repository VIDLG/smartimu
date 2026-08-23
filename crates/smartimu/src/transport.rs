use crate::{
    BusInfo, DeviceEvent, DeviceHeader, DeviceMessage, DeviceResponse, ErrorEvent,
    GetImuDeviceInfoRequest, GetInventoryRequest, GetPowerStatusRequest, HeartbeatEvent,
    HostHeader, HostRequest, ImuDeviceInfo, ImuDeviceInfoPayload, ImuDeviceInfoResponse, ImuId,
    ImuSelection, InventoryPayload, InventoryResponse, LowPowerSeverity, MessageSeq,
    PROTOCOL_VERSION, PingRequest, PongPayload, PongResponse, PowerEvent, PowerEventPayload,
    PowerStatus, PowerStatusResponse, ProbeDetectedEvent, ProbeDetectedPayload, ProtocolError,
    RawImuSample, RawSampleEvent, RawSamplePayload, ResponseResult, SampleIndex,
    SampleReadoutRequest, SessionId, SmartImuError, StartSamplingPayload, StartSamplingRequest,
    StartSamplingResponse, StopSamplingPayload, StopSamplingRequest, StopSamplingResponse,
    SystemId, SystemInfo, TimestampUs,
};
use alloc::string::String;
use alloc::vec::Vec;

pub struct DeviceSession {
    pub system_id: SystemId,
    /// Identifies one boot/run of a system. The board layer should create this
    /// value because it knows what entropy, RTC, or persistent counter is available.
    pub session_id: SessionId,
    seq: MessageSeq,
}

impl DeviceSession {
    pub const fn new(system_id: SystemId, session_id: SessionId) -> Self {
        Self {
            system_id,
            session_id,
            seq: MessageSeq(0),
        }
    }

    pub fn header(&mut self, timestamp_us: TimestampUs) -> DeviceHeader {
        let header = DeviceHeader {
            protocol_version: PROTOCOL_VERSION,
            system_id: self.system_id,
            session_id: self.session_id,
            seq: self.seq,
            timestamp_us,
        };
        self.seq = self.seq.wrapping_next();
        header
    }

    pub fn pong(&mut self, timestamp_us: TimestampUs, message: &str) -> DeviceMessage {
        DeviceMessage::Response(DeviceResponse::Pong(PongResponse {
            header: self.header(timestamp_us),
            result: ResponseResult::Ok(PongPayload {
                message: String::from(message),
            }),
        }))
    }

    pub fn inventory_response(
        &mut self,
        timestamp_us: TimestampUs,
        system_label: &str,
        buses: Vec<BusInfo>,
        imu_devices: Vec<ImuDeviceInfo>,
    ) -> DeviceMessage {
        DeviceMessage::Response(DeviceResponse::Inventory(InventoryResponse {
            header: self.header(timestamp_us),
            result: ResponseResult::Ok(InventoryPayload {
                system: SystemInfo {
                    system_id: self.system_id,
                    label: String::from(system_label),
                },
                buses,
                imu_devices,
            }),
        }))
    }

    pub fn imu_device_info_response(
        &mut self,
        timestamp_us: TimestampUs,
        imu_id: ImuId,
        info: ImuDeviceInfo,
    ) -> DeviceMessage {
        DeviceMessage::Response(DeviceResponse::ImuDeviceInfo(ImuDeviceInfoResponse {
            header: self.header(timestamp_us),
            result: ResponseResult::Ok(ImuDeviceInfoPayload { imu_id, info }),
        }))
    }

    pub fn imu_device_info_not_found_response(
        &mut self,
        timestamp_us: TimestampUs,
        _imu_id: ImuId,
    ) -> DeviceMessage {
        DeviceMessage::Response(DeviceResponse::ImuDeviceInfo(ImuDeviceInfoResponse {
            header: self.header(timestamp_us),
            result: ResponseResult::Err(
                ProtocolError::from(SmartImuError::ImuNotFound).with_imu_id(Some(_imu_id)),
            ),
        }))
    }

    pub fn power_status_response(
        &mut self,
        timestamp_us: TimestampUs,
        status: PowerStatus,
    ) -> DeviceMessage {
        DeviceMessage::Response(DeviceResponse::PowerStatus(PowerStatusResponse {
            header: self.header(timestamp_us),
            result: ResponseResult::Ok(status),
        }))
    }

    pub fn start_sampling_response(
        &mut self,
        timestamp_us: TimestampUs,
        imu_ids: Vec<ImuId>,
    ) -> DeviceMessage {
        DeviceMessage::Response(DeviceResponse::StartSampling(StartSamplingResponse {
            header: self.header(timestamp_us),
            result: ResponseResult::Ok(StartSamplingPayload { imu_ids }),
        }))
    }

    pub fn stop_sampling_response(
        &mut self,
        timestamp_us: TimestampUs,
        imu_ids: Vec<ImuId>,
    ) -> DeviceMessage {
        DeviceMessage::Response(DeviceResponse::StopSampling(StopSamplingResponse {
            header: self.header(timestamp_us),
            result: ResponseResult::Ok(StopSamplingPayload { imu_ids }),
        }))
    }

    pub fn probe_detected(
        &mut self,
        timestamp_us: TimestampUs,
        imu_id: ImuId,
        driver_id: &str,
        spi_profile: crate::SpiProfile,
        chip_info: crate::DetectedChipInfo,
    ) -> DeviceMessage {
        DeviceMessage::Event(DeviceEvent::ProbeDetected(ProbeDetectedEvent {
            header: self.header(timestamp_us),
            payload: ProbeDetectedPayload {
                imu_id,
                driver_id: crate::DriverId(String::from(driver_id)),
                spi_profile,
                chip_info,
            },
        }))
    }

    pub fn raw_sample(
        &mut self,
        timestamp_us: TimestampUs,
        imu_id: ImuId,
        sample_index: SampleIndex,
        sample_timestamp_us: TimestampUs,
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
        timestamp_us: TimestampUs,
        imu_id: Option<ImuId>,
        error: SmartImuError,
    ) -> DeviceMessage {
        DeviceMessage::Event(DeviceEvent::Error(ErrorEvent {
            header: self.header(timestamp_us),
            payload: ProtocolError::from(error).with_imu_id(imu_id),
        }))
    }

    pub fn power_status(
        &mut self,
        timestamp_us: TimestampUs,
        status: PowerStatus,
    ) -> DeviceMessage {
        DeviceMessage::Event(DeviceEvent::Power(PowerEvent {
            header: self.header(timestamp_us),
            payload: PowerEventPayload::Status(status),
        }))
    }

    pub fn low_power(
        &mut self,
        timestamp_us: TimestampUs,
        status: PowerStatus,
        severity: LowPowerSeverity,
    ) -> DeviceMessage {
        DeviceMessage::Event(DeviceEvent::Power(PowerEvent {
            header: self.header(timestamp_us),
            payload: PowerEventPayload::LowPower { status, severity },
        }))
    }

    pub fn heartbeat(
        &mut self,
        timestamp_us: TimestampUs,
        active_imu_ids: Vec<ImuId>,
    ) -> DeviceMessage {
        DeviceMessage::Event(DeviceEvent::Heartbeat(HeartbeatEvent {
            header: self.header(timestamp_us),
            payload: crate::HeartbeatPayload { active_imu_ids },
        }))
    }
}

#[derive(Default)]
pub struct HostClient {
    seq: MessageSeq,
}

impl HostClient {
    pub fn header(&mut self) -> HostHeader {
        let header = HostHeader {
            protocol_version: PROTOCOL_VERSION,
            seq: self.seq,
        };
        self.seq = self.seq.wrapping_next();
        header
    }

    pub fn ping(&mut self, message: &str) -> HostRequest {
        HostRequest::Ping(PingRequest {
            header: self.header(),
            message: String::from(message),
        })
    }

    pub fn get_inventory(&mut self) -> HostRequest {
        HostRequest::GetInventory(GetInventoryRequest {
            header: self.header(),
        })
    }

    pub fn get_imu_device_info(&mut self, imu_id: ImuId) -> HostRequest {
        HostRequest::GetImuDeviceInfo(GetImuDeviceInfoRequest {
            header: self.header(),
            imu_id,
        })
    }

    pub fn get_power_status(&mut self) -> HostRequest {
        HostRequest::GetPowerStatus(GetPowerStatusRequest {
            header: self.header(),
        })
    }

    pub fn start_sampling(
        &mut self,
        selection: ImuSelection,
        sample_request: SampleReadoutRequest,
    ) -> HostRequest {
        HostRequest::StartSampling(StartSamplingRequest {
            header: self.header(),
            selection,
            sample_request,
        })
    }

    pub fn stop_sampling(&mut self, selection: ImuSelection) -> HostRequest {
        HostRequest::StopSampling(StopSamplingRequest {
            header: self.header(),
            selection,
        })
    }
}
