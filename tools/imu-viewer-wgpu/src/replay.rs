use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use smartimu::{WireFrame, decode_json};

use crate::state::frame_uptime_ms;

#[derive(Clone, Copy, Debug)]
pub struct ReplayClock {
    pub first_uptime_ms: u32,
    pub base_instant: Instant,
}

impl ReplayClock {
    pub fn new(frame: &WireFrame) -> Self {
        Self {
            first_uptime_ms: frame_uptime_ms(frame),
            base_instant: Instant::now(),
        }
    }

    pub fn due(&self, frame: &WireFrame) -> bool {
        let elapsed_ms = self.base_instant.elapsed().as_millis() as u32;
        frame_uptime_ms(frame).saturating_sub(self.first_uptime_ms) <= elapsed_ms
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

pub fn load_replay_frames(path: &Path) -> Result<Vec<WireFrame>, String> {
    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut frames = Vec::new();
    for (line_index, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match decode_json(trimmed) {
            Ok(frame) => frames.push(frame),
            Err(error) => return Err(format!("line {}: {}", line_index + 1, error)),
        }
    }
    Ok(frames)
}
