use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;

use crate::pty::{PtyManager, PtySpawnSpec};

const READY_TIMEOUT: Duration = Duration::from_secs(90);
const READY_INTERVAL: Duration = Duration::from_millis(400);
const HEALTH_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceState {
    Starting,
    Ready,
    Unreachable,
    Stopped,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ServiceSpec {
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub detected_from: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Service {
    pub key: String,
    pub repository_id: String,
    pub worktree: String,
    pub port: u16,
    pub session_id: String,
    pub state: ServiceState,
    pub command: String,
    pub detected_from: String,
    pub url: String,
}

pub struct ServiceRegistry {
    manager: Arc<PtyManager>,
    services: Mutex<BTreeMap<String, Service>>,
}

pub fn detect_service(worktree: &Path, port: u16) -> Result<ServiceSpec> {
    let package_json = worktree.join("package.json");
    if package_json.exists() {
        let raw = fs::read_to_string(&package_json)?;
        let manifest: serde_json::Value = serde_json::from_str(&raw)?;

        let has_dependency = |name: &str| {
            ["dependencies", "devDependencies"].iter().any(|section| {
                manifest
                    .get(section)
                    .and_then(|value| value.get(name))
                    .is_some()
            })
        };
        let has_script = |name: &str| {
            manifest
                .get("scripts")
                .and_then(|value| value.get(name))
                .is_some()
        };

        let mut env = BTreeMap::new();
        env.insert("PORT".to_owned(), port.to_string());
        env.insert("BROWSER".to_owned(), "none".to_owned());

        if has_dependency("vite") && has_script("dev") {
            return Ok(ServiceSpec {
                command: "npm".to_owned(),
                args: vec![
                    "run".to_owned(),
                    "dev".to_owned(),
                    "--".to_owned(),
                    "--port".to_owned(),
                    port.to_string(),
                    "--strictPort".to_owned(),
                ],
                env,
                detected_from: "package.json (vite)".to_owned(),
            });
        }

        if has_script("dev") {
            return Ok(ServiceSpec {
                command: "npm".to_owned(),
                args: vec!["run".to_owned(), "dev".to_owned()],
                env,
                detected_from: "package.json (dev script)".to_owned(),
            });
        }

        if has_script("start") {
            return Ok(ServiceSpec {
                command: "npm".to_owned(),
                args: vec!["start".to_owned()],
                env,
                detected_from: "package.json (start script)".to_owned(),
            });
        }
    }

    if worktree.join("Cargo.toml").exists() {
        let mut env = BTreeMap::new();
        env.insert("PORT".to_owned(), port.to_string());
        return Ok(ServiceSpec {
            command: "cargo".to_owned(),
            args: vec!["run".to_owned()],
            env,
            detected_from: "Cargo.toml".to_owned(),
        });
    }

    bail!("no service detected in {}", worktree.display())
}

async fn port_open(port: u16) -> bool {
    TcpStream::connect(("127.0.0.1", port)).await.is_ok()
}

impl ServiceRegistry {
    pub fn new(manager: Arc<PtyManager>) -> Arc<Self> {
        Arc::new(Self {
            manager,
            services: Mutex::new(BTreeMap::new()),
        })
    }

    pub fn list(&self) -> Vec<Service> {
        self.services.lock().values().cloned().collect()
    }

    pub fn start(
        self: &Arc<Self>,
        repository_id: &str,
        worktree_name: &str,
        worktree_path: &Path,
        port: u16,
    ) -> Result<Service> {
        let key = format!("{repository_id}/{worktree_name}");
        if let Some(existing) = self.services.lock().get(&key) {
            if existing.state != ServiceState::Stopped {
                bail!("service already running for {key}");
            }
        }

        let spec = detect_service(worktree_path, port)?;
        let session = self.manager.spawn(PtySpawnSpec {
            command: spec.command.clone(),
            args: spec.args.clone(),
            cwd: Some(worktree_path.to_string_lossy().to_string()),
            env: spec.env.clone(),
            cols: 120,
            rows: 32,
        })?;

        let service = Service {
            key: key.clone(),
            repository_id: repository_id.to_owned(),
            worktree: worktree_name.to_owned(),
            port,
            session_id: session.id.clone(),
            state: ServiceState::Starting,
            command: format!("{} {}", spec.command, spec.args.join(" ")),
            detected_from: spec.detected_from,
            url: format!("http://127.0.0.1:{port}"),
        };

        self.services.lock().insert(key.clone(), service.clone());
        self.clone().watch(key, port);

        Ok(service)
    }

    fn watch(self: Arc<Self>, key: String, port: u16) {
        tokio::spawn(async move {
            let deadline = tokio::time::Instant::now() + READY_TIMEOUT;

            loop {
                if tokio::time::Instant::now() > deadline {
                    self.set_state(&key, ServiceState::Unreachable);
                    return;
                }

                if port_open(port).await {
                    break;
                }

                if self.state_of(&key) == Some(ServiceState::Stopped) {
                    return;
                }

                tokio::time::sleep(READY_INTERVAL).await;
            }

            self.set_state(&key, ServiceState::Ready);

            loop {
                tokio::time::sleep(HEALTH_INTERVAL).await;

                match self.state_of(&key) {
                    None | Some(ServiceState::Stopped) => return,
                    _ => {}
                }

                let state = if port_open(port).await {
                    ServiceState::Ready
                } else {
                    ServiceState::Unreachable
                };
                self.set_state(&key, state);
            }
        });
    }

    fn state_of(&self, key: &str) -> Option<ServiceState> {
        self.services.lock().get(key).map(|service| service.state)
    }

    fn set_state(&self, key: &str, state: ServiceState) {
        if let Some(service) = self.services.lock().get_mut(key) {
            service.state = state;
        }
    }

    pub fn stop(&self, repository_id: &str, worktree_name: &str) -> Result<()> {
        let key = format!("{repository_id}/{worktree_name}");
        let service = self
            .services
            .lock()
            .get(&key)
            .cloned()
            .ok_or_else(|| anyhow!("no service for {key}"))?;

        let _ = self.manager.remove(&service.session_id);
        self.set_state(&key, ServiceState::Stopped);
        self.services.lock().remove(&key);

        Ok(())
    }
}
