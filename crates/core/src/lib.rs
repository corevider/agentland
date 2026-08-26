pub mod bench;
pub mod metrics;
pub mod ports;
pub mod repo;
pub mod pty;
pub mod server;

pub use bench::GeneratorSpec;
pub use metrics::{MetricsStore, Sample};
pub use ports::PortRegistry;
pub use repo::{RepoRegistry, Repository, Worktree, WorktreeStatus};
pub use pty::{PtyManager, PtySpawnSpec, Session, SessionInfo};
pub use server::{serve, ServerConfig};

pub fn generate_token() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    let pid = std::process::id() as u128;
    format!("{:032x}", nanos.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(pid))
}
