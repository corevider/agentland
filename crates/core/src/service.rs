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
        format!("http://{}:{}", connectable(&self.host), self.port)
    }
}

/// An address something can actually connect to.
///
/// A core told to serve everybody binds 0.0.0.0, which is not an address: it
/// means "every one of them". Announced as it was written, a window read it
/// back and tried to fetch http://0.0.0.0:9470, which fails with nothing more
/// helpful than "Load failed". Anything on this machine reaches it at the
/// loopback address, whatever it was told to bind.
pub fn connectable(host: &str) -> &str {
    match host {
        "0.0.0.0" | "::" | "[::]" | "" => "127.0.0.1",
        held => held,
    }
}

/// The addresses this machine answers on, for a phone on the same network.
///
/// Found by asking the routing table which address it would use to reach the
/// outside world — no packet is sent, and nothing is guessed from a list of
/// interfaces that may all be down.
pub fn on_this_network(port: u16) -> Vec<String> {
    let mut hosts = Vec::new();

    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(here) = socket.local_addr() {
                hosts.push(format!("{}:{port}", here.ip()));
            }
        }
    }

    if let Ok(name) = std::env::var("HOSTNAME") {
        if !name.trim().is_empty() {
            hosts.push(format!("{}:{port}", name.trim()));
        }
    }

    hosts
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
    fn this_machine_can_say_where_a_phone_should_look() {
        let hosts = on_this_network(9470);

        // A machine with no network is a machine with nothing to offer here,
        // and that is not an error — but each answer must carry the port.
        for host in &hosts {
            assert!(host.ends_with(":9470"), "{host} must name the port");
        }
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
    fn the_address_everybody_is_served_on_is_not_one_to_dial() {
        assert_eq!(connectable("0.0.0.0"), "127.0.0.1");
        assert_eq!(connectable("::"), "127.0.0.1");
        assert_eq!(connectable(""), "127.0.0.1");
        assert_eq!(connectable("192.168.1.5"), "192.168.1.5");
        assert_eq!(connectable("127.0.0.1"), "127.0.0.1");
    }

    #[test]
    fn what_is_announced_is_dialled_at_the_loopback_when_it_was_bound_to_all() {
        let held = Endpoint {
            host: "0.0.0.0".into(),
            port: 9470,
            token: String::new(),
            pid: 1,
        };

        assert_eq!(held.url(), "http://127.0.0.1:9470");
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
