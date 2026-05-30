use std::collections::HashMap;

use smartimu::{
    DeviceFrame, ImuId, ImuNodeInfo, ImuSampleConfig, OrientationFrame, RangeDps, RangeG,
    SampleConfigOptions, SampleFrame, SampleRateHz,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    Raw6Axis,
    Quaternion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackState {
    Playing,
    Paused,
}

impl PlaybackState {
    pub fn toggle(&mut self) {
        *self = match self {
            Self::Playing => Self::Paused,
            Self::Paused => Self::Playing,
        };
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Playing => "playing",
            Self::Paused => "paused",
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct IntegratedOrientation {
    pub roll: f32,
    pub pitch: f32,
    pub yaw: f32,
    pub last_sample_timestamp_us: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct ViewerState {
    pub imu_infos: HashMap<ImuId, ImuNodeInfo>,
    pub latest_samples: HashMap<ImuId, SampleFrame>,
    pub latest_orientation: HashMap<ImuId, OrientationFrame>,
    pub interpolated_orientation: HashMap<ImuId, OrientationFrame>,
    pub integrated_orientation: HashMap<ImuId, IntegratedOrientation>,
    pub selected_imu: Option<ImuId>,
    pub view_mode: ViewMode,
    pub active_imus: u16,
    pub last_seq: Option<u32>,
    pub received_frames: u64,
    pub dropped_seq_count: u64,
}

impl Default for ViewerState {
    fn default() -> Self {
        Self {
            imu_infos: HashMap::new(),
            latest_samples: HashMap::new(),
            latest_orientation: HashMap::new(),
            interpolated_orientation: HashMap::new(),
            integrated_orientation: HashMap::new(),
            selected_imu: None,
            view_mode: ViewMode::Quaternion,
            active_imus: 0,
            last_seq: None,
            received_frames: 0,
            dropped_seq_count: 0,
        }
    }
}

impl ViewerState {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn toggle_view_mode(&mut self) {
        self.view_mode = match self.view_mode {
            ViewMode::Raw6Axis => ViewMode::Quaternion,
            ViewMode::Quaternion => ViewMode::Raw6Axis,
        };
    }

    pub fn sorted_imu_ids(&self) -> Vec<ImuId> {
        let mut ids: Vec<ImuId> = self.imu_infos.keys().copied().collect();
        if ids.is_empty() {
            ids.extend(self.latest_samples.keys().copied());
            ids.extend(self.latest_orientation.keys().copied());
            ids.sort_by_key(|imu_id| (imu_id.system_id, imu_id.sensor_id));
            ids.dedup();
            return ids;
        }
        ids.sort_by_key(|imu_id| (imu_id.system_id, imu_id.sensor_id));
        ids
    }

    pub fn select_next_imu(&mut self) {
        let ids = self.sorted_imu_ids();
        if ids.is_empty() {
            self.selected_imu = None;
            return;
        }
        self.selected_imu = Some(match self.selected_imu {
            None => ids[0],
            Some(current) => {
                let index = ids
                    .iter()
                    .position(|imu_id| *imu_id == current)
                    .unwrap_or(0);
                ids[(index + 1) % ids.len()]
            }
        });
    }

    pub fn clear_selection(&mut self) {
        self.selected_imu = None;
    }

    pub fn handle_frame(&mut self, frame: DeviceFrame) {
        self.received_frames = self.received_frames.wrapping_add(1);
        match frame {
            DeviceFrame::Ping(frame) => self.update_seq(frame.header.seq),
            DeviceFrame::Inventory(frame) => {
                self.update_seq(frame.header.seq);
                self.imu_infos.clear();
                for info in frame.imus {
                    self.imu_infos.insert(info.id, info);
                }
            }
            DeviceFrame::ImuNodeInfo(frame) => {
                self.update_seq(frame.header.seq);
                if let Some(info) = frame.info {
                    self.imu_infos.insert(info.id, info);
                }
            }
            DeviceFrame::ProbeResult(frame) => self.update_seq(frame.header.seq),
            DeviceFrame::Sample(frame) => {
                self.update_seq(frame.header.seq);
                self.update_integrated_orientation(&frame);
                self.imu_infos
                    .entry(frame.imu_id)
                    .or_insert_with(|| synthesize_info(&frame));
                self.latest_samples.insert(frame.imu_id, frame);
            }
            DeviceFrame::Orientation(frame) => {
                self.update_seq(frame.header.seq);
                self.latest_orientation.insert(frame.imu_id, frame);
            }
            DeviceFrame::Error(frame) => self.update_seq(frame.header.seq),
            DeviceFrame::Heartbeat(frame) => {
                self.update_seq(frame.header.seq);
                self.active_imus = frame.active_imus;
            }
        }
    }

    pub fn update_interpolated_orientations(&mut self) -> bool {
        let mut changed = false;
        for (imu_id, orientation) in &self.latest_orientation {
            let current = orientation.clone();
            let entry = self
                .interpolated_orientation
                .entry(*imu_id)
                .or_insert_with(|| current.clone());
            let before = entry.quaternion;
            *entry = OrientationFrame {
                header: current.header,
                imu_id: current.imu_id,
                imu_chip: current.imu_chip,
                sample_index: current.sample_index,
                sample_timestamp_us: current.sample_timestamp_us,
                quaternion: nlerp_quaternion(entry.quaternion, current.quaternion, 0.35),
            };
            changed |= quaternion_distance(before, entry.quaternion) > 0.000_001;
        }
        changed
    }

    fn update_seq(&mut self, seq: u32) {
        if let Some(last) = self.last_seq {
            let expected = last.wrapping_add(1);
            if seq != expected {
                self.dropped_seq_count = self.dropped_seq_count.wrapping_add(1);
            }
        }
        self.last_seq = Some(seq);
    }

    fn update_integrated_orientation(&mut self, sample: &SampleFrame) {
        const GYRO_DPS_PER_LSB: f32 = 1.0 / 16.0;
        let state = self
            .integrated_orientation
            .entry(sample.imu_id)
            .or_default();
        let dt = if let Some(last) = state.last_sample_timestamp_us {
            ((sample.sample_timestamp_us.saturating_sub(last)) as f32 / 1_000_000.0).clamp(0.0, 0.1)
        } else {
            0.0
        };
        state.last_sample_timestamp_us = Some(sample.sample_timestamp_us);
        state.roll += (sample.sample.imu6.gyro[0] as f32 * GYRO_DPS_PER_LSB).to_radians() * dt;
        state.pitch += (sample.sample.imu6.gyro[1] as f32 * GYRO_DPS_PER_LSB).to_radians() * dt;
        state.yaw += (sample.sample.imu6.gyro[2] as f32 * GYRO_DPS_PER_LSB).to_radians() * dt;
    }
}

pub fn frame_emit_timestamp_us(frame: &DeviceFrame) -> u64 {
    match frame {
        DeviceFrame::Ping(frame) => frame.header.emit_timestamp_us,
        DeviceFrame::Inventory(frame) => frame.header.emit_timestamp_us,
        DeviceFrame::ImuNodeInfo(frame) => frame.header.emit_timestamp_us,
        DeviceFrame::ProbeResult(frame) => frame.header.emit_timestamp_us,
        DeviceFrame::Sample(frame) => frame.header.emit_timestamp_us,
        DeviceFrame::Orientation(frame) => frame.header.emit_timestamp_us,
        DeviceFrame::Error(frame) => frame.header.emit_timestamp_us,
        DeviceFrame::Heartbeat(frame) => frame.header.emit_timestamp_us,
    }
}

pub fn nlerp_quaternion(
    a: smartimu::Quaternion,
    b: smartimu::Quaternion,
    t: f32,
) -> smartimu::Quaternion {
    let mut b = b;
    let dot = a.w * b.w + a.x * b.x + a.y * b.y + a.z * b.z;
    if dot < 0.0 {
        b.w = -b.w;
        b.x = -b.x;
        b.y = -b.y;
        b.z = -b.z;
    }
    let one_minus_t = 1.0 - t;
    let w = one_minus_t * a.w + t * b.w;
    let x = one_minus_t * a.x + t * b.x;
    let y = one_minus_t * a.y + t * b.y;
    let z = one_minus_t * a.z + t * b.z;
    let norm = (w * w + x * x + y * y + z * z).sqrt().max(1e-6);
    smartimu::Quaternion {
        w: w / norm,
        x: x / norm,
        y: y / norm,
        z: z / norm,
    }
}

fn quaternion_distance(a: smartimu::Quaternion, b: smartimu::Quaternion) -> f32 {
    let dw = a.w - b.w;
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    let dz = a.z - b.z;
    dw * dw + dx * dx + dy * dy + dz * dz
}

fn synthesize_info(sample: &SampleFrame) -> ImuNodeInfo {
    ImuNodeInfo {
        id: sample.imu_id,
        bus_id: smartimu::BusId(0),
        chip_profile: fallback_chip_profile(sample.imu_chip),
        label: None,
        sample_config: fallback_sample_config(),
    }
}

fn fallback_sample_config() -> ImuSampleConfig {
    ImuSampleConfig {
        accel_range: RangeG(2),
        gyro_range: RangeDps(2048),
        sample_rate_hz: SampleRateHz(100),
    }
}

fn fallback_sample_config_options() -> SampleConfigOptions {
    SampleConfigOptions::Constrained {
        configs: std::borrow::Cow::Owned(vec![fallback_sample_config()]),
    }
}

fn fallback_chip_profile(chip: smartimu::ImuChip) -> smartimu::ImuChipProfile {
    smartimu::ImuChipProfile {
        chip,
        sample_config_options: fallback_sample_config_options(),
        sample_readout_support: smartimu::SampleReadoutSupport::default(),
        temperature_config: None,
    }
}
