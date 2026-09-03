use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Where the core is listening, written down so anything else can find it.
///
/// The core used to live inside the window: closing it, or rebuilding it, took
/// every pane with it — measured all through a day's work, where each rebuild
/// killed the agents mid-turn. A core that outlives the window has to be
/// findable by the next window, by a terminal, by anything.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    pub token: String,
    /// The process serving it, so a stale file can be told from a live one.
    pub pid: u32,
}

impl Endpoint {
    pub fn url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }
}

fn file(data_dir: &Path) -> PathBuf {
    data_dir.join("service.json")
}

/// Say where this core is listening.
pub fn announce(data_dir: &Path, endpoint: &Endpoint) {
    let _ = std::fs::create_dir_all(data_dir);

    if let Ok(body) = serde_json::to_vec_pretty(endpoint) {
        let _ = std::fs::write(file(data_dir), body);
    }
}

/// What was announced, if anything. Says nothing about whether it still answers
/// — a file outlives the process that wrote it, so the caller knocks.
pub fn announced(data_dir: &Path) -> Option<Endpoint> {
    let raw = std::fs::read(file(data_dir)).ok()?;
    serde_json::from_slice(&raw).ok()
}

pub fn forget(data_dir: &Path) {
    let _ = std::fs::remove_file(file(data_dir));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("agentland-service-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn what_was_announced_is_what_is_read_back() {
        let dir = scratch("round-trip");
        let held = Endpoint {
            host: "127.0.0.1".into(),
            port: 9470,
            token: "a-token".into(),
            pid: 4242,
        };

        announce(&dir, &held);

        assert_eq!(announced(&dir), Some(held));
    }

    #[test]
    fn nothing_announced_is_not_an_error() {
        assert_eq!(announced(&scratch("empty")), None);
    }

    #[test]
    fn a_file_that_is_not_an_endpoint_is_ignored_rather_than_believed() {
        let dir = scratch("rubbish");
        std::fs::write(file(&dir), b"half a file").unwrap();

        assert_eq!(announced(&dir), None);
    }

    #[test]
    fn forgetting_leaves_nothing_to_find() {
        let dir = scratch("forget");
        announce(
            &dir,
            &Endpoint {
                host: "127.0.0.1".into(),
                port: 1,
                token: String::new(),
                pid: 1,
            },
        );

        forget(&dir);

        assert_eq!(announced(&dir), None);
    }
}
