use std::collections::{HashMap, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use bytes::{Bytes, BytesMut};
use parking_lot::Mutex;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

const FRAME_INTERVAL: Duration = Duration::from_millis(8);
const FRAME_MAX_BYTES: usize = 32 * 1024;
const READ_BUFFER_BYTES: usize = 64 * 1024;
const CHANNEL_CAPACITY: usize = 512;
const REPLAY_MAX_BYTES: usize = 256 * 1024;

#[derive(Default)]
struct Replay {
    frames: VecDeque<Bytes>,
    bytes: usize,
}

impl Replay {
    fn push(&mut self, frame: Bytes) {
        self.bytes += frame.len();
        self.frames.push_back(frame);

        while self.bytes > REPLAY_MAX_BYTES {
            match self.frames.pop_front() {
                Some(dropped) => self.bytes -= dropped.len(),
                None => break,
            }
        }
    }

    fn snapshot(&self) -> Vec<Bytes> {
        self.frames.iter().cloned().collect()
    }
}

pub struct Broadcaster {
    sender: broadcast::Sender<Bytes>,
    replay: Mutex<Replay>,
    stats: Mutex<SessionStats>,
}

impl Broadcaster {
    fn new() -> Arc<Self> {
        let (sender, _) = broadcast::channel(CHANNEL_CAPACITY);
        let started = now_secs();
        Arc::new(Self {
            sender,
            replay: Mutex::new(Replay::default()),
            stats: Mutex::new(SessionStats {
                started_at: started,
                last_output_at: started,
                ..SessionStats::default()
            }),
        })
    }

    pub fn publish(&self, frame: Bytes) {
        {
            let mut stats = self.stats.lock();
            stats.last_output_at = now_secs();
            stats.bytes += frame.len() as u64;
            stats.lines += frame.iter().filter(|byte| **byte == b'\n').count() as u64;
        }

        let mut replay = self.replay.lock();
        replay.push(frame.clone());
        let _ = self.sender.send(frame);
    }

    pub fn stats(&self) -> SessionStats {
        *self.stats.lock()
    }

    pub fn subscribe(&self) -> (Vec<Bytes>, broadcast::Receiver<Bytes>) {
        let replay = self.replay.lock();
        let receiver = self.sender.subscribe();
        (replay.snapshot(), receiver)
    }
}

fn default_cols() -> u16 {
    120
}

fn default_rows() -> u16 {
    32
}

#[derive(Clone, Debug, Deserialize)]
pub struct PtySpawnSpec {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    #[serde(default = "default_cols")]
    pub cols: u16,
    #[serde(default = "default_rows")]
    pub rows: u16,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct SessionStats {
    pub started_at: u64,
    pub last_output_at: u64,
    pub bytes: u64,
    pub lines: u64,
    pub context_percent: Option<u8>,
    pub context_tokens: Option<u64>,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or_default()
}

#[derive(Clone, Debug, Serialize)]
pub struct SessionInfo {
    pub id: String,
    pub kind: &'static str,
    pub command: String,
    pub cols: u16,
    pub rows: u16,
    #[serde(default)]
    pub cwd: Option<String>,
}

struct PtyHandles {
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
}

pub struct Session {
    info: Mutex<SessionInfo>,
    broadcaster: Arc<Broadcaster>,
    handles: Option<PtyHandles>,
}

impl Session {
    pub fn subscribe(&self) -> (Vec<Bytes>, broadcast::Receiver<Bytes>) {
        self.broadcaster.subscribe()
    }

    pub fn info(&self) -> SessionInfo {
        self.info.lock().clone()
    }

    pub fn stats(&self) -> SessionStats {
        self.broadcaster.stats()
    }

    pub fn alive(&self) -> bool {
        match self.handles.as_ref() {
            Some(handles) => handles
                .child
                .lock()
                .try_wait()
                .map(|status| status.is_none())
                .unwrap_or(false),
            None => true,
        }
    }

    pub fn write_input(&self, data: &[u8]) -> Result<()> {
        let handles = self
            .handles
            .as_ref()
            .ok_or_else(|| anyhow!("session does not accept input"))?;
        let mut writer = handles.writer.lock();
        writer.write_all(data)?;
        writer.flush()?;
        Ok(())
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        if let Some(handles) = self.handles.as_ref() {
            handles.master.lock().resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })?;
        }
        let mut info = self.info.lock();
        info.cols = cols;
        info.rows = rows;
        Ok(())
    }

    pub fn kill(&self) -> Result<()> {
        if let Some(handles) = self.handles.as_ref() {
            let mut child = handles.child.lock();
            let _ = child.kill();
            let _ = child.wait();
        }
        Ok(())
    }
}

pub struct PtyManager {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    next_id: Mutex<u64>,
    log_dir: PathBuf,
    boot: String,
}

impl PtyManager {
    pub fn new() -> Self {
        Self::with_log_dir(PathBuf::from("sessions"))
    }

    pub fn with_log_dir(log_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&log_dir);
        let boot = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_secs())
            .unwrap_or_default();

        Self {
            sessions: Mutex::new(HashMap::new()),
            next_id: Mutex::new(1),
            log_dir,
            boot: format!("{boot:x}"),
        }
    }

    fn open_log(&self, id: &str) -> Option<BufWriter<File>> {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.log_dir.join(format!("{id}.log")))
            .ok()
            .map(BufWriter::new)
    }

    pub fn read_log(&self, id: &str, bytes: u64) -> Result<Vec<u8>> {
        let path = self.log_dir.join(format!("{id}.log"));
        let mut file = File::open(&path)?;
        let length = file.metadata()?.len();
        let start = length.saturating_sub(bytes);
        file.seek(SeekFrom::Start(start))?;

        let mut buffer = Vec::with_capacity((length - start) as usize);
        file.read_to_end(&mut buffer)?;
        Ok(buffer)
    }

    fn allocate_id(&self, prefix: &str) -> String {
        let mut next = self.next_id.lock();
        let id = format!("{prefix}-{}-{}", self.boot, *next);
        *next += 1;
        id
    }

    pub fn spawn(&self, spec: PtySpawnSpec) -> Result<SessionInfo> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows: spec.rows,
            cols: spec.cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let mut command = CommandBuilder::new(&spec.command);
        command.args(&spec.args);
        if let Some(cwd) = &spec.cwd {
            command.cwd(cwd);
        }
        command.env("TERM", "xterm-256color");
        for (name, value) in &spec.env {
            command.env(name, value);
        }

        let child = pair.slave.spawn_command(command)?;
        drop(pair.slave);

        let reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        let broadcaster = Broadcaster::new();

        let id = self.allocate_id("pane");
        let log = self.open_log(&id);
        let info = SessionInfo {
            id: id.clone(),
            kind: "pty",
            command: spec.command.clone(),
            cols: spec.cols,
            rows: spec.rows,
            cwd: spec.cwd.clone(),
        };

        let session = Arc::new(Session {
            info: Mutex::new(info.clone()),
            broadcaster: broadcaster.clone(),
            handles: Some(PtyHandles {
                writer: Mutex::new(writer),
                master: Mutex::new(pair.master),
                child: Mutex::new(child),
            }),
        });

        spawn_reader(reader, broadcaster, log);
        self.sessions.lock().insert(id, session);

        Ok(info)
    }

    pub fn spawn_generator(&self, spec: crate::bench::GeneratorSpec) -> Result<SessionInfo> {
        let broadcaster = Broadcaster::new();
        let id = self.allocate_id("bench");

        let info = SessionInfo {
            id: id.clone(),
            kind: "generator",
            command: format!("generator {} lines/s", spec.lines_per_second),
            cols: default_cols(),
            rows: default_rows(),
            cwd: None,
        };

        crate::bench::spawn_generator(spec, broadcaster.clone());

        let session = Arc::new(Session {
            info: Mutex::new(info.clone()),
            broadcaster,
            handles: None,
        });

        self.sessions.lock().insert(id, session);
        Ok(info)
    }

    pub fn get(&self, id: &str) -> Option<Arc<Session>> {
        self.sessions.lock().get(id).cloned()
    }

    pub fn list(&self) -> Vec<SessionInfo> {
        self.sessions
            .lock()
            .values()
            .map(|session| session.info())
            .collect()
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        let session = self
            .sessions
            .lock()
            .remove(id)
            .ok_or_else(|| anyhow!("unknown session: {id}"))?;
        session.kill()
    }
}

impl Default for PtyManager {
    fn default() -> Self {
        Self::new()
    }
}

fn spawn_reader(
    mut reader: Box<dyn Read + Send>,
    broadcaster: Arc<Broadcaster>,
    mut log: Option<BufWriter<File>>,
) {
    std::thread::spawn(move || {
        let mut buffer = vec![0u8; READ_BUFFER_BYTES];
        let mut pending = BytesMut::with_capacity(FRAME_MAX_BYTES);
        let mut last_flush = Instant::now();

        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    if let Some(writer) = log.as_mut() {
                        let _ = writer.write_all(&buffer[..count]);
                    }

                    pending.extend_from_slice(&buffer[..count]);

                    let burst_drained = count < buffer.len();
                    let interval_elapsed = last_flush.elapsed() >= FRAME_INTERVAL;
                    let frame_full = pending.len() >= FRAME_MAX_BYTES;

                    if burst_drained || interval_elapsed || frame_full {
                        broadcaster.publish(pending.split().freeze());
                        if let Some(writer) = log.as_mut() {
                            let _ = writer.flush();
                        }
                        last_flush = Instant::now();
                    }
                }
                Err(_) => break,
            }
        }

        if !pending.is_empty() {
            broadcaster.publish(pending.freeze());
        }

        if let Some(writer) = log.as_mut() {
            let _ = writer.flush();
        }
    });
}
