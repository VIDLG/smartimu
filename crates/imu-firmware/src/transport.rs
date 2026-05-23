use alloc::string::{String, ToString};
use alloc::vec::Vec;
use imu_core::{
    BusDescriptor, ErrorFrame, HeartbeatFrame, HelloFrame, ImuChip, ImuDescriptor, ImuError, ImuId,
    MAX_MESSAGE_LEN, PROTOCOL_VERSION, ProbeResultFrame, RawSample, SampleFrame, TopologyFrame,
    WireFormat, WireFrame, WireHeader,
};

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

    pub fn header(&mut self, uptime_ms: u32) -> WireHeader {
        let header = WireHeader {
            protocol_version: PROTOCOL_VERSION,
            format: self.format,
            system_id: self.system_id,
            session_id: self.session_id,
            seq: self.seq,
            uptime_ms,
        };
        self.seq = self.seq.wrapping_add(1);
        header
    }

    pub fn hello(&mut self, uptime_ms: u32, system_label: &str) -> WireFrame {
        WireFrame::Hello(HelloFrame {
            header: self.header(uptime_ms),
            system_label: bounded_string(system_label, imu_core::MAX_LABEL_LEN),
        })
    }

    pub fn topology(
        &mut self,
        uptime_ms: u32,
        buses: Vec<BusDescriptor>,
        imus: Vec<ImuDescriptor>,
    ) -> WireFrame {
        WireFrame::Topology(TopologyFrame {
            header: self.header(uptime_ms),
            buses,
            imus,
        })
    }

    pub fn probe_result(
        &mut self,
        uptime_ms: u32,
        imu_id: ImuId,
        driver_name: &str,
        detected_chip: ImuChip,
        success: bool,
        error: Option<ImuError>,
        profile: Option<imu_core::SpiProfile>,
    ) -> WireFrame {
        WireFrame::ProbeResult(ProbeResultFrame {
            header: self.header(uptime_ms),
            imu_id,
            driver_name: bounded_string(driver_name, imu_core::MAX_LABEL_LEN),
            detected_chip,
            success,
            error,
            profile,
        })
    }

    pub fn sample(
        &mut self,
        uptime_ms: u32,
        imu_id: ImuId,
        imu_chip: ImuChip,
        sample_index: u32,
        timestamp_us: u64,
        sample: RawSample,
        status_bits: u16,
    ) -> WireFrame {
        WireFrame::Sample(SampleFrame {
            header: self.header(uptime_ms),
            imu_id,
            imu_chip,
            sample_index,
            timestamp_us,
            sample,
            status_bits,
        })
    }

    pub fn error(
        &mut self,
        uptime_ms: u32,
        imu_id: Option<ImuId>,
        error: ImuError,
        message: &str,
    ) -> WireFrame {
        WireFrame::Error(ErrorFrame {
            header: self.header(uptime_ms),
            imu_id,
            error,
            message: bounded_string(message, MAX_MESSAGE_LEN),
        })
    }

    pub fn heartbeat(&mut self, uptime_ms: u32, active_imus: u16) -> WireFrame {
        WireFrame::Heartbeat(HeartbeatFrame {
            header: self.header(uptime_ms),
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
