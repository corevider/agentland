pub mod approvals;
pub mod auth;
pub mod bench;
pub mod board;
pub mod brief;
pub mod context;
pub mod crew;
pub mod db;
pub mod dispatch;
pub mod embed;
pub mod files;
pub mod gateway;
pub mod mail;
pub mod memory;
pub mod notices;
pub mod metrics;
pub mod plans;
pub mod ports;
pub mod pulls;
pub mod repo;
pub mod routines;
pub mod services;
pub mod stacks;
pub mod start;
pub mod pty;
pub mod server;
pub mod workspaces;
pub mod skills;
pub mod supervisor;
pub mod transcript;
pub mod vault;

pub use approvals::{AnswerApproval, Approval, Approvals, RequestApproval};
pub use auth::{Scope as TokenScope, ScopedToken, TokenStore};
pub use bench::GeneratorSpec;
pub use board::{Board, Column, Task};
pub use crew::{Agent, Crew, Engine, HireRequest};
pub use context::{read_context, ContextReading};
pub use db::Database;
pub use plans::{DraftPlan, Plan, PlanState, Plans, Step, StepState};
pub use embed::{EmbedderReport, EmbedderSettings};
pub use skills::{Skill, SkillLibrary};
pub use supervisor::{Observation, Supervisor, Verdict, Watch};
pub use workspaces::{CreateWorkspace, Workspace, Workspaces};
pub use dispatch::{decide, Caps, Decision, Dispatch, DispatchState};
pub use gateway::{CallRequest, ConnectRequest, Gateway, Integration};
pub use mail::{Mailbox, MailPolicy, Message, SendMessage};
pub use memory::{mask_secrets, Memory, MemoryStore, ProposeMemory};
pub use metrics::{MetricsStore, Sample};
pub use ports::PortRegistry;
pub use pulls::{where_it_stands, PullState, Standing};
pub use repo::{Commit, RepoRegistry, Repository, Worktree, WorktreeStatus};
pub use routines::{CreateRoutine, Routine, Routines};
pub use services::{Service, ServiceRegistry, ServiceState};
pub use stacks::{starter, Starter, CATALOG as STARTERS};
pub use start::{commander_name, engine_for_a_commander, worktree_name};
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
