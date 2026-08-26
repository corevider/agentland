pub mod approvals;
pub mod auth;
pub mod bench;
pub mod board;
pub mod crew;
pub mod db;
pub mod dispatch;
pub mod gateway;
pub mod mail;
pub mod memory;
pub mod metrics;
pub mod ports;
pub mod repo;
pub mod routines;
pub mod services;
pub mod pty;
pub mod server;

pub use approvals::{AnswerApproval, Approval, Approvals, RequestApproval};
pub use auth::{Scope as TokenScope, ScopedToken, TokenStore};
pub use bench::GeneratorSpec;
pub use board::{Board, Column, Task};
pub use crew::{Agent, Crew, Engine, HireRequest};
pub use db::Database;
pub use dispatch::{decide, Caps, Decision, DispatchState};
pub use gateway::{CallRequest, ConnectRequest, Gateway, Integration};
pub use mail::{Mailbox, MailPolicy, Message, SendMessage};
pub use memory::{mask_secrets, Memory, MemoryStore, ProposeMemory, Scope};
pub use metrics::{MetricsStore, Sample};
pub use ports::PortRegistry;
pub use repo::{RepoRegistry, Repository, Worktree, WorktreeStatus};
pub use routines::{CreateRoutine, Routine, Routines};
pub use services::{Service, ServiceRegistry, ServiceState};
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
