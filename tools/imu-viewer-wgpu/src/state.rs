use std::collections::HashMap;

use imu_core::{ImuDescriptor, ImuId, OrientationFrame, SampleFrame, WireFrame};

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
    pub last_timestamp_us: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct ViewerState {
    pub topology: HashMap<ImuId, ImuDescriptor>,
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
            topology: HashMap::new(),
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
        let mut ids: Vec<ImuId> = self.topology.keys().copied().collect();
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

    pub fn handle_frame(&mut self, frame: WireFrame) {
        self.received_frames = self.received_frames.wrapping_add(1);
        match frame {
            WireFrame::Hello(frame) => self.update_seq(frame.header.seq),
            WireFrame::Topology(frame) => {
                self.update_seq(frame.header.seq);
                self.topology.clear();
                for descriptor in frame.imus {
                    self.topology.insert(descriptor.id, descriptor);
                }
            }
            WireFrame::ProbeResult(frame) => self.update_seq(frame.header.seq),
            WireFrame::Sample(frame) => {
                self.update_seq(frame.header.seq);
                self.update_integrated_orientation(&frame);
                self.topology
                    .entry(frame.imu_id)
                    .or_insert_with(|| synthesize_descriptor(&frame));
                self.latest_samples.insert(frame.imu_id, frame);
            }
            WireFrame::Orientation(frame) => {
                self.update_seq(frame.header.seq);
                self.latest_orientation.insert(frame.imu_id, frame);
            }
            WireFrame::Error(frame) => self.update_seq(frame.header.seq),
            WireFrame::Heartbeat(frame) => {
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
                timestamp_us: current.timestamp_us,
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
        let dt = if let Some(last) = state.last_timestamp_us {
            ((sample.timestamp_us.saturating_sub(last)) as f32 / 1_000_000.0).clamp(0.0, 0.1)
        } else {
            0.0
        };
        state.last_timestamp_us = Some(sample.timestamp_us);
        state.roll += (sample.sample.gyro[0] as f32 * GYRO_DPS_PER_LSB).to_radians() * dt;
        state.pitch += (sample.sample.gyro[1] as f32 * GYRO_DPS_PER_LSB).to_radians() * dt;
        state.yaw += (sample.sample.gyro[2] as f32 * GYRO_DPS_PER_LSB).to_radians() * dt;
    }
}

pub fn frame_uptime_ms(frame: &WireFrame) -> u32 {
    match frame {
        WireFrame::Hello(frame) => frame.header.uptime_ms,
        WireFrame::Topology(frame) => frame.header.uptime_ms,
        WireFrame::ProbeResult(frame) => frame.header.uptime_ms,
        WireFrame::Sample(frame) => frame.header.uptime_ms,
        WireFrame::Orientation(frame) => frame.header.uptime_ms,
        WireFrame::Error(frame) => frame.header.uptime_ms,
        WireFrame::Heartbeat(frame) => frame.header.uptime_ms,
    }
}

pub fn nlerp_quaternion(
    a: imu_core::Quaternion,
    b: imu_core::Quaternion,
    t: f32,
) -> imu_core::Quaternion {
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
    imu_core::Quaternion {
        w: w / norm,
        x: x / norm,
        y: y / norm,
        z: z / norm,
    }
}

fn quaternion_distance(a: imu_core::Quaternion, b: imu_core::Quaternion) -> f32 {
    let dw = a.w - b.w;
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    let dz = a.z - b.z;
    dw * dw + dx * dx + dy * dy + dz * dz
}

fn synthesize_descriptor(sample: &SampleFrame) -> ImuDescriptor {
    ImuDescriptor {
        id: sample.imu_id,
        bus_id: imu_core::BusId(0),
        chip: sample.imu_chip,
        label: format!("imu-{}", sample.imu_id.sensor_id),
        sample_config: None,
        supported_sample_configs: Vec::new(),
    }
}
