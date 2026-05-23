use crate::{
    BusInfo, DeviceFrame, DeviceHeader, ErrorFrame, HeartbeatFrame, ImuChip, ImuId, ImuInfo,
    ImuInfoFrame, InventoryFrame, MAX_MESSAGE_LEN, PROTOCOL_VERSION, PingFrame, ProbeResult,
    ProbeResultFrame, RawImuSample, SampleFrame, SmartImuError, SystemInfo, WireFormat,
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

    pub fn header(&mut self, emit_timestamp_us: u64) -> DeviceHeader {
        let header = DeviceHeader {
            protocol_version: PROTOCOL_VERSION,
            format: self.format,
            system_id: self.system_id,
            session_id: self.session_id,
            seq: self.seq,
            emit_timestamp_us,
        };
        self.seq = self.seq.wrapping_add(1);
        header
    }

    pub fn ping(&mut self, emit_timestamp_us: u64, message: &str) -> DeviceFrame {
        DeviceFrame::Ping(PingFrame {
            header: self.header(emit_timestamp_us),
            message: bounded_string(message, crate::MAX_MESSAGE_LEN),
        })
    }

    pub fn inventory(
        &mut self,
        emit_timestamp_us: u64,
        system_label: &str,
        buses: Vec<BusInfo>,
        imus: Vec<ImuInfo>,
    ) -> DeviceFrame {
        DeviceFrame::Inventory(InventoryFrame {
            header: self.header(emit_timestamp_us),
            system: SystemInfo {
                system_id: self.system_id,
                label: bounded_string(system_label, crate::MAX_LABEL_LEN),
            },
            buses,
            imus,
        })
    }

    pub fn imu_info(
        &mut self,
        emit_timestamp_us: u64,
        imu_id: ImuId,
        info: Option<ImuInfo>,
    ) -> DeviceFrame {
        DeviceFrame::ImuInfo(ImuInfoFrame {
            header: self.header(emit_timestamp_us),
            imu_id,
            info,
        })
    }

    pub fn probe_result(
        &mut self,
        emit_timestamp_us: u64,
        imu_id: ImuId,
        probe_label: &str,
        result: ProbeResult,
    ) -> DeviceFrame {
        DeviceFrame::ProbeResult(ProbeResultFrame {
            header: self.header(emit_timestamp_us),
            imu_id,
            probe_label: bounded_string(probe_label, crate::MAX_LABEL_LEN),
            result,
        })
    }

    pub fn sample(
        &mut self,
        emit_timestamp_us: u64,
        imu_id: ImuId,
        imu_chip: ImuChip,
        sample_index: u32,
        sample_timestamp_us: u64,
        sample: RawImuSample,
        status_bits: u16,
    ) -> DeviceFrame {
        DeviceFrame::Sample(SampleFrame {
            header: self.header(emit_timestamp_us),
            imu_id,
            imu_chip,
            sample_index,
            sample_timestamp_us,
            sample,
            status_bits,
        })
    }

    pub fn error(
        &mut self,
        emit_timestamp_us: u64,
        imu_id: Option<ImuId>,
        error: SmartImuError,
        message: &str,
    ) -> DeviceFrame {
        DeviceFrame::Error(ErrorFrame {
            header: self.header(emit_timestamp_us),
            imu_id,
            error,
            message: bounded_string(message, MAX_MESSAGE_LEN),
        })
    }

    pub fn heartbeat(&mut self, emit_timestamp_us: u64, active_imus: u16) -> DeviceFrame {
        DeviceFrame::Heartbeat(HeartbeatFrame {
            header: self.header(emit_timestamp_us),
            active_imus,
        })
    }
}

pub fn bounded_string(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

pub fn protocol_string(value: &str) -> String {
    value.to_string()
}
