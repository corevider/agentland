use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

const MAX_SAMPLES: usize = 512;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Sample {
    pub run_id: String,
    pub elapsed_ms: u64,
    pub panes: u32,
    pub lines_per_second: u32,
    pub fps: u32,
    pub worst_frame_ms: u32,
    pub mb_per_second: f64,
    pub dropped_frames: u64,
    pub dropped_local: u64,
    pub renderer: String,
    pub surface: String,
    #[serde(default)]
    pub gpu: String,
}

pub struct MetricsStore {
    samples: Mutex<Vec<Sample>>,
    path: PathBuf,
}

impl MetricsStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            samples: Mutex::new(Vec::new()),
            path,
        }
    }

    pub fn record(&self, sample: Sample) {
        if let Ok(line) = serde_json::to_string(&sample) {
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&self.path) {
                let _ = writeln!(file, "{line}");
            }
        }

        let mut samples = self.samples.lock();
        samples.push(sample);
        if samples.len() > MAX_SAMPLES {
            let overflow = samples.len() - MAX_SAMPLES;
            samples.drain(0..overflow);
        }
    }

    pub fn samples(&self) -> Vec<Sample> {
        self.samples.lock().clone()
    }
}
