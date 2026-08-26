use std::time::Duration;

use std::sync::Arc;

use bytes::{Bytes, BytesMut};
use serde::Deserialize;

use crate::pty::Broadcaster;

const TICK: Duration = Duration::from_millis(8);

fn default_lines_per_second() -> u32 {
    10_000
}

fn default_duration_ms() -> u64 {
    30_000
}

fn default_line_width() -> usize {
    96
}

#[derive(Clone, Debug, Deserialize)]
pub struct GeneratorSpec {
    #[serde(default = "default_lines_per_second")]
    pub lines_per_second: u32,
    #[serde(default = "default_duration_ms")]
    pub duration_ms: u64,
    #[serde(default = "default_line_width")]
    pub line_width: usize,
    #[serde(default)]
    pub colored: bool,
}

impl Default for GeneratorSpec {
    fn default() -> Self {
        Self {
            lines_per_second: default_lines_per_second(),
            duration_ms: default_duration_ms(),
            line_width: default_line_width(),
            colored: true,
        }
    }
}

pub fn spawn_generator(spec: GeneratorSpec, broadcaster: Arc<Broadcaster>) {
    tokio::spawn(async move {
        let ticks_per_second = (1000 / TICK.as_millis().max(1)) as u32;
        let lines_per_tick = (spec.lines_per_second / ticks_per_second).max(1);
        let total_ticks = spec.duration_ms / TICK.as_millis() as u64;
        let mut interval = tokio::time::interval(TICK);
        let mut line_number: u64 = 0;

        for _ in 0..total_ticks {
            interval.tick().await;

            let mut frame = BytesMut::with_capacity(lines_per_tick as usize * (spec.line_width + 16));
            for _ in 0..lines_per_tick {
                line_number += 1;
                append_line(&mut frame, line_number, &spec);
            }

            broadcaster.publish(frame.freeze());
        }

        broadcaster.publish(Bytes::from_static(b"\r\n[generator finished]\r\n"));
    });
}

fn append_line(frame: &mut BytesMut, line_number: u64, spec: &GeneratorSpec) {
    let payload = format!("line {line_number:>9} ");
    let filler_len = spec.line_width.saturating_sub(payload.len());

    if spec.colored {
        let color = 31 + (line_number % 6) as u8;
        frame.extend_from_slice(format!("\x1b[{color}m").as_bytes());
    }

    frame.extend_from_slice(payload.as_bytes());
    for index in 0..filler_len {
        frame.extend_from_slice(&[b'a' + ((line_number as usize + index) % 26) as u8]);
    }

    if spec.colored {
        frame.extend_from_slice(b"\x1b[0m");
    }

    frame.extend_from_slice(b"\r\n");
}
