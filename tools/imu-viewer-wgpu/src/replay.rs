use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use smartimu::{DeviceFrame, WireFrame, decode_json};

use crate::state::frame_emit_timestamp_us;

#[derive(Clone, Copy, Debug)]
pub struct ReplayClock {
    pub first_emit_timestamp_us: u64,
    pub base_instant: Instant,
}

impl ReplayClock {
    pub fn new(frame: &DeviceFrame) -> Self {
        Self {
            first_emit_timestamp_us: frame_emit_timestamp_us(frame),
            base_instant: Instant::now(),
        }
    }

    pub fn due(&self, frame: &DeviceFrame) -> bool {
        let elapsed_us = self.base_instant.elapsed().as_micros() as u64;
        frame_emit_timestamp_us(frame).saturating_sub(self.first_emit_timestamp_us) <= elapsed_us
    }
}

pub fn find_default_replay_path() -> Option<PathBuf> {
    let root = std::env::current_dir().ok()?;
    let mut candidates = Vec::new();
    for entry in fs::read_dir(root).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
            continue;
        }
        let metadata = entry.metadata().ok()?;
        let modified = metadata.modified().ok()?;
        candidates.push((modified, path));
    }
    candidates.sort_by_key(|(modified, _)| *modified);
    candidates.pop().map(|(_, path)| path)
}

pub fn load_replay_frames(path: &Path) -> Result<Vec<DeviceFrame>, String> {
    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut frames = Vec::new();
    for (line_index, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match decode_json(trimmed).ok().and_then(wire_to_device) {
            Some(frame) => frames.push(frame),
            None => return Err(format!("line {}: invalid device frame", line_index + 1)),
        }
    }
    Ok(frames)
}

fn wire_to_device(frame: WireFrame) -> Option<DeviceFrame> {
    match frame {
        WireFrame::Device(frame) => Some(frame),
        WireFrame::Host(_) => None,
    }
}
