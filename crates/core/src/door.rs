//! The door a phone comes in by, opened and closed from the window.
//!
//! The core answers the machine it runs on. A phone on the same network
//! needs it to answer that network too, and for a long time the only way was
//! to start the core again with `AGENTLAND_HOST=0.0.0.0` — which stops every
//! agent mid-turn. This opens a second door instead: the core is bound to
//! this machine's own network address, on the plain port and the secure one,
//! while the loopback door stays where it was. Closing lets those go. The
//! choice is remembered, so a core started later opens it on its own.

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use axum::Router;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DoorState {
    /// Open by how the core was started, and not this module's to close.
    Config,
    Open,
    Closed,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct Wanted {
    #[serde(default)]
    open: bool,
}

struct Opened {
    /// The `host:port` entries added to what the guard accepts.
    allowed: Vec<String>,
    handles: Vec<axum_server::Handle>,
}

pub struct Door {
    data_dir: PathBuf,
    port: u16,
    by_config: bool,
    allowed: Arc<RwLock<Vec<String>>>,
    served: Mutex<Option<Router>>,
    opened: Mutex<Option<Opened>>,
}

impl Door {
    pub fn new(data_dir: PathBuf, port: u16, by_config: bool, allowed: Arc<RwLock<Vec<String>>>) -> Self {
        Self {
            data_dir,
            port,
            by_config,
            allowed,
            served: Mutex::new(None),
            opened: Mutex::new(None),
        }
    }

    /// The router to answer with, once it is built.
    pub fn serve_with(&self, app: Router) {
        *self.served.lock() = Some(app);
    }

    /// Whether a person asked for the door to be open, last time anyone asked.
    pub fn wanted(&self) -> bool {
        let held: Wanted = crate::db::load_state(&self.data_dir, "door");
        held.open
    }

    fn remember(&self, open: bool) {
        crate::db::save_state(&self.data_dir, "door", &Wanted { open });
    }

    pub fn state(&self) -> DoorState {
        if self.by_config {
            DoorState::Config
        } else if self.opened.lock().is_some() {
            DoorState::Open
        } else {
            DoorState::Closed
        }
    }

    /// Bind this machine's network addresses, plain and secure, and let the
    /// guard accept them.
    pub async fn open(&self) -> Result<Vec<String>> {
        if self.by_config {
            bail!("the core was started answering the network, so the door is open already");
        }
        if self.opened.lock().is_some() {
            return Ok(self.allowed_now());
        }

        let app = self
            .served
            .lock()
            .clone()
            .ok_or_else(|| anyhow!("the core is not serving yet"))?;

        let on_the_network = crate::service::on_this_network(self.port);
        let addresses = addresses_in(&on_the_network);
        if addresses.is_empty() {
            bail!("this machine has no network address for a phone to reach");
        }

        let allowed = allowed_for(&on_the_network, self.port);
        let papers = crate::tls::papers_for(&self.data_dir, &allowed)?;
        let secure = axum_server::tls_rustls::RustlsConfig::from_pem_file(papers.certificate, papers.key)
            .await
            .map_err(|error| anyhow!("cannot read this machine's papers: {error}"))?;

        let mut handles = Vec::new();
        for ip in addresses {
            for (addr, with_tls) in [
                (SocketAddr::new(ip, self.port), false),
                (SocketAddr::new(ip, self.port + 1), true),
            ] {
                let handle = axum_server::Handle::new();
                let served = app.clone();
                let watched = handle.clone();
                let config = secure.clone();

                tokio::spawn(async move {
                    let outcome = if with_tls {
                        axum_server::bind_rustls(addr, config)
                            .handle(watched)
                            .serve(served.into_make_service())
                            .await
                    } else {
                        axum_server::bind(addr)
                            .handle(watched)
                            .serve(served.into_make_service())
                            .await
                    };
                    if let Err(error) = outcome {
                        tracing::warn!(%addr, %error, "the phone's door closed on its own");
                    }
                });

                if handle.listening().await.is_none() {
                    for made in &handles {
                        let held: &axum_server::Handle = made;
                        held.shutdown();
                    }
                    bail!("cannot listen on {addr} — is another core already answering there?");
                }
                tracing::info!(%addr, "the phone's door is open");
                handles.push(handle);
            }
        }

        {
            let mut held = self.allowed.write();
            for entry in &allowed {
                if !held.contains(entry) {
                    held.push(entry.clone());
                }
            }
        }

        *self.opened.lock() = Some(Opened {
            allowed: allowed.clone(),
            handles,
        });
        self.remember(true);
        Ok(allowed)
    }

    /// Let the network addresses go. The loopback door stays.
    pub fn close(&self) -> Result<()> {
        if self.by_config {
            bail!("the core was started answering the network — start it without AGENTLAND_HOST to close that");
        }

        if let Some(opened) = self.opened.lock().take() {
            for handle in &opened.handles {
                handle.shutdown();
            }
            let mut held = self.allowed.write();
            held.retain(|entry| !opened.allowed.contains(entry));
            tracing::info!("the phone's door is closed");
        }

        self.remember(false);
        Ok(())
    }

    fn allowed_now(&self) -> Vec<String> {
        self.opened
            .lock()
            .as_ref()
            .map(|held| held.allowed.clone())
            .unwrap_or_default()
    }
}

/// The addresses among this machine's `host:port` entries that are addresses
/// and not names: a socket binds an address, and a name is whatever the
/// network says it is.
pub fn addresses_in(hosts: &[String]) -> Vec<IpAddr> {
    let mut found: Vec<IpAddr> = hosts
        .iter()
        .filter_map(|held| held.rsplit_once(':').map(|(host, _)| host))
        .filter_map(|host| host.parse::<IpAddr>().ok())
        .filter(|ip| !ip.is_loopback() && !ip.is_unspecified())
        .collect();
    found.dedup();
    found
}

/// What the guard should accept once the door is open: every `host:port`
/// this machine goes by, on the plain port and the secure one after it.
pub fn allowed_for(hosts: &[String], port: u16) -> Vec<String> {
    let mut allowed = Vec::new();
    for held in hosts {
        let Some((host, _)) = held.rsplit_once(':') else {
            continue;
        };
        for at in [port, port + 1] {
            let entry = format!("{}:{at}", host.to_ascii_lowercase());
            if !allowed.contains(&entry) {
                allowed.push(entry);
            }
        }
    }
    allowed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_addresses_are_bound_and_never_the_loopback() {
        let found = addresses_in(&[
            "192.168.1.128:9470".into(),
            "ege-laptop:9470".into(),
            "127.0.0.1:9470".into(),
            "0.0.0.0:9470".into(),
        ]);
        assert_eq!(found, vec!["192.168.1.128".parse::<IpAddr>().unwrap()]);
    }

    #[test]
    fn the_guard_learns_both_ports_for_every_name() {
        let allowed = allowed_for(&["192.168.1.128:9470".into(), "Ege-Laptop:9470".into()], 9470);
        assert_eq!(
            allowed,
            vec![
                "192.168.1.128:9470",
                "192.168.1.128:9471",
                "ege-laptop:9470",
                "ege-laptop:9471"
            ]
        );
    }

    #[test]
    fn a_door_started_open_by_configuration_is_not_this_modules_to_close() {
        let dir = std::env::temp_dir().join("agentland-door-config");
        let _ = std::fs::remove_dir_all(&dir);
        let door = Door::new(dir, 9470, true, Arc::new(RwLock::new(Vec::new())));
        assert_eq!(door.state(), DoorState::Config);
        assert!(door.close().is_err());
    }

    #[test]
    fn the_wish_is_remembered() {
        let dir = std::env::temp_dir().join("agentland-door-wish");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let door = Door::new(dir.clone(), 9470, false, Arc::new(RwLock::new(Vec::new())));
        assert!(!door.wanted());
        door.remember(true);
        assert!(Door::new(dir, 9470, false, Arc::new(RwLock::new(Vec::new()))).wanted());
    }
}
