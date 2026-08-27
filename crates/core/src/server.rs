use std::net::SocketAddr;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::error::RecvError;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::ServeDir;

use crate::approvals::{AnswerApproval, Approval, Approvals, RequestApproval};
use crate::auth::{permits as scope_permits, Scope as TokenScope, TokenStore};
use crate::bench::GeneratorSpec;
use crate::context::{read_context, ContextReading};
use crate::board::{Board, Column, CreateTask, Evidence, MoveTask, Task};
use crate::crew::{Agent, Crew, Engine, HireRequest};
use crate::dispatch::{Decision, Dispatch, DispatchState};
use crate::embed::{EmbedderReport, EmbedderSettings};
use crate::gateway::{CallRequest, ConnectRequest, Gateway, Integration};
use crate::mail::{MailPolicy, Mailbox, Message as MailMessage, SendMessage};
use crate::memory::{Memory, MemoryStore, ProposeMemory, Recalled, Scope};
use crate::plans::{DraftPlan, Plan, Plans, StepState};
use crate::routines::{CreateRoutine, Routine, Routines};
use crate::metrics::{MetricsStore, Sample};
use crate::repo::{Commit, PullRequest, RepoRegistry, Repository, Review, Worktree, WorktreeStatus};
use crate::services::{Service, ServiceRegistry};
use crate::skills::{Skill, SkillLibrary};
use crate::supervisor::{judge, safe_to_type, should_reap, Observation, Supervisor, Verdict, Watch};
use crate::workspaces::{CreateWorkspace, Workspace, Workspaces};
use crate::pty::{PtyManager, PtySpawnSpec, SessionInfo, SessionStats};

#[derive(Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub token: String,
    pub allowed_hosts: Vec<String>,
    pub allowed_origins: Vec<String>,
    pub data_dir: PathBuf,
}

impl ServerConfig {
    pub fn data_dir_from_env() -> PathBuf {
        std::env::var("AGENTLAND_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("data"))
    }
}

#[derive(Clone)]
struct AppState {
    manager: Arc<PtyManager>,
    config: Arc<ServerConfig>,
    metrics: Arc<MetricsStore>,
    repos: Arc<RepoRegistry>,
    services: Arc<ServiceRegistry>,
    crew: Arc<Crew>,
    board: Arc<Board>,
    dispatch: Arc<Dispatch>,
    memories: Arc<MemoryStore>,
    mail: Arc<Mailbox>,
    routines: Arc<Routines>,
    gateway: Arc<Gateway>,
    approvals: Arc<Approvals>,
    tokens: Arc<TokenStore>,
    skills: Arc<SkillLibrary>,
    plans: Arc<Plans>,
    supervisor: Arc<Supervisor>,
    embedder: Arc<parking_lot::Mutex<EmbedderSettings>>,
    data_dir: Arc<PathBuf>,
    workspaces: Arc<Workspaces>,
    ui_commands: Arc<parking_lot::Mutex<Vec<String>>>,
}

#[derive(Deserialize)]
struct TokenQuery {
    #[serde(default)]
    token: Option<String>,
}

#[derive(Deserialize)]
struct InputBody {
    data: String,
}

#[derive(Deserialize)]
struct ResizeBody {
    cols: u16,
    rows: u16,
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

pub async fn serve(manager: Arc<PtyManager>, config: ServerConfig) -> Result<()> {
    let manager_for_services = manager.clone();
    let manager_for_crew = manager.clone();
    let port_for_crew = config.port;
    let token_for_crew = config.token.clone();
    let token_for_store = config.token.clone();
    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;

    let origins: Vec<HeaderValue> = config
        .allowed_origins
        .iter()
        .filter_map(|origin| origin.parse().ok())
        .collect();
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
        .allow_headers([
            header::CONTENT_TYPE,
            HeaderName::from_static("x-auth-token"),
        ]);

    let data_dir = config.data_dir.clone();
    let state = AppState {
        manager,
        config: Arc::new(config),
        metrics: Arc::new(MetricsStore::new(data_dir.join("bench-results.jsonl"))),
        repos: Arc::new(RepoRegistry::new(data_dir.clone())),
        services: ServiceRegistry::new(manager_for_services),
        crew: {
            let crew = Crew::new(manager_for_crew, data_dir.clone());
            crew.set_endpoint(port_for_crew, token_for_crew);
            crew
        },
        board: Arc::new(Board::new(data_dir.clone())),
        dispatch: Arc::new(Dispatch::new(data_dir.clone())),
        memories: Arc::new(MemoryStore::new(data_dir.clone())),
        mail: Arc::new(Mailbox::new(data_dir.clone())),
        routines: Arc::new(Routines::new(data_dir.clone())),
        gateway: Arc::new(Gateway::new(data_dir.clone())),
        approvals: Arc::new(Approvals::new(data_dir.clone())),
        tokens: Arc::new(TokenStore::new(token_for_store, data_dir.clone())),
        skills: Arc::new(SkillLibrary::new(data_dir.clone())),
        plans: Arc::new(Plans::new(data_dir.clone())),
        supervisor: Arc::new(Supervisor::new(data_dir.clone())),
        embedder: Arc::new(parking_lot::Mutex::new(crate::embed::load(&data_dir))),
        data_dir: Arc::new(data_dir.clone()),
        workspaces: Arc::new(Workspaces::new(data_dir.clone())),
        ui_commands: Arc::new(parking_lot::Mutex::new(Vec::new())),
    };

    spawn_routine_ticker(state.clone());
    spawn_supervisor(state.clone());

    let app = Router::new()
        .route("/sessions", get(list_sessions).post(spawn_session))
        .route("/sessions/{id}", delete(kill_session))
        .route("/sessions/{id}/input", post(write_input))
        .route("/sessions/{id}/resize", post(resize_session))
        .route("/sessions/{id}/stream", get(stream_session))
        .route("/sessions/{id}/log", get(read_log))
        .route("/sessions/{id}/stats", get(read_stats))
        .route("/bench", post(spawn_generator))
        .route("/metrics", get(read_metrics).post(record_metrics))
        .route("/repos", get(list_repos).post(add_repo))
        .route("/repos/{id}/worktrees", get(list_worktrees).post(create_worktree))
        .route("/repos/{id}/worktrees/{name}", delete(remove_worktree))
        .route("/ports", get(list_ports))
        .route("/services", get(list_services))
        .route("/engines", get(list_engines))
        .route("/agents", get(list_agents).post(hire_agent))
        .route("/agents/{id}", delete(dismiss_agent))
        .route("/agents/{id}/start", post(start_agent))
        .route("/agents/{id}/stop", post(stop_agent))
        .route("/tasks", get(list_tasks).post(create_task))
        .route("/tasks/{id}", delete(delete_task))
        .route("/tasks/{id}/move", post(move_task))
        .route("/tasks/{id}/assign", post(assign_task))
        .route("/dispatch", get(dispatch_status))
        .route("/dispatch/pause", post(pause_dispatch))
        .route("/dispatch/caps", post(set_caps))
        .route("/memories", get(list_memories).post(propose_memory))
        .route("/memories/search", get(search_memories))
        .route("/memories/embedder", get(read_embedder).post(set_embedder))
        .route("/memories/{id}", delete(forget_memory))
        .route("/memories/{id}/approve", post(approve_memory))
        .route("/mail", get(list_mail).post(send_mail))
        .route("/mail/policy", get(mail_policy).post(set_mail_policy))
        .route("/routines", get(list_routines).post(create_routine))
        .route("/routines/{id}", delete(delete_routine))
        .route("/routines/{id}/enabled", post(set_routine_enabled))
        .route("/integrations", get(list_integrations).post(connect_integration))
        .route("/integrations/{id}", delete(disconnect_integration))
        .route("/integrations/call", post(call_integration))
        .route("/approvals", get(list_approvals).post(request_approval))
        .route("/approvals/{id}", post(answer_approval))
        .route("/workspaces", get(list_workspaces).post(create_workspace))
        .route("/workspaces/{id}", delete(remove_workspace).post(set_workspace_repos))
        .route("/workspaces/active", post(activate_workspace))
        .route("/plans", get(list_plans).post(create_plan))
        .route("/plans/{id}", get(read_plan).delete(abandon_plan))
        .route("/plans/{id}/steps/{step}", post(mark_step))
        .route("/plans/ready", get(ready_steps))
        .route("/supervisor", get(supervisor_status))
        .route("/skills", get(list_skills).post(write_skill))
        .route("/skills/{id}", delete(remove_skill))
        .route(
            "/agents/{id}/skills",
            get(list_agent_skills).post(install_skill),
        )
        .route("/agents/{id}/skills/{skill_id}", delete(uninstall_skill))
        .route("/devices", get(list_devices).post(pair_device))
        .route("/devices/{id}", delete(revoke_device))
        .route("/ui/commands", get(take_ui_commands).post(queue_ui_command))
        .route("/dispatch/tasks/{id}", post(dispatch_task))
        .route("/repos/{id}/worktrees/{name}/review", get(review_worktree))
        .route("/repos/{id}/worktrees/{name}/commit", post(commit_worktree))
        .route("/repos/{id}/worktrees/{name}/pr", post(open_pull_request))
        .route(
            "/repos/{id}/worktrees/{name}/service",
            post(start_service).delete(stop_service),
        )
        .with_state(state.clone());

    let app = match std::env::var("AGENTLAND_MOBILE_DIR").ok().filter(|dir| !dir.is_empty()) {
        Some(dir) => {
            tracing::info!(%dir, "serving the phone companion at /mobile");
            app.nest_service("/mobile", ServeDir::new(dir))
        }
        None => app,
    };

    let app = match std::env::var("AGENTLAND_UI_DIR").ok().filter(|dir| !dir.is_empty()) {
        Some(dir) => {
            tracing::info!(%dir, "serving the interface");
            app.fallback_service(ServeDir::new(dir))
        }
        None => app,
    };

    let app = app
        .layer(middleware::from_fn_with_state(state.clone(), guard))
        .layer(cors);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "core listening");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn guard(
    State(state): State<AppState>,
    Query(query): Query<TokenQuery>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if !state.config.allowed_hosts.iter().any(|allowed| allowed == &host) {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorBody {
                error: "unexpected Host header".into(),
            }),
        )
            .into_response();
    }

    if is_public_asset(request.uri().path()) {
        return next.run(request).await;
    }

    let header_token = headers
        .get("x-auth-token")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let token = header_token.or(query.token).unwrap_or_default();

    let Some(scope) = state.tokens.resolve(&token) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ErrorBody {
                error: "invalid token".into(),
            }),
        )
            .into_response();
    };

    if !scope_permits(scope, request.method().as_str(), request.uri().path()) {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorBody {
                error: "this device is limited to reading and answering approvals".into(),
            }),
        )
            .into_response();
    }

    next.run(request).await
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or_default()
}

fn identity_for(state: &AppState, agent: &Agent) -> Option<String> {
    if agent.role != "commander" {
        return None;
    }

    let crew: Vec<String> = state
        .crew
        .list()
        .into_iter()
        .filter(|entry| entry.id != agent.id)
        .map(|entry| format!("{} ({}, {})", entry.name, entry.role, entry.engine_id))
        .collect();

    let roster = if crew.is_empty() {
        "Nobody is hired yet — say so rather than planning work for agents that do not exist."
            .to_owned()
    } else {
        format!("The crew you can hand steps to: {}.", crew.join("; "))
    };

    Some(format!(
        "You are {}, the commander of this crew. You plan and delegate; you do not edit code.\n         {roster}\n         Your tools are plan_create, plan_ready, plan_status, plan_step_done and crew_delegate.          Start by reading the board with task_list.",
        agent.name
    ))
}

const BRIEF_MEMORIES: usize = 6;

async fn compose_brief(state: &AppState, agent: &Agent, base: &str) -> String {
    let vector = embed_text(state, base.to_owned()).await;

    let learned = state
        .memories
        .recall(
            Scope::Repository,
            &agent.repository_id,
            base,
            vector.as_deref(),
            state.embedder.lock().min_similarity,
            BRIEF_MEMORIES,
        )
        .into_iter()
        .map(|found| found.memory.text)
        .collect();

    let mail = state
        .mail
        .take_inbox(&agent.id)
        .into_iter()
        .map(|message| (message.from, message.text))
        .collect();

    crate::brief::compose(crate::brief::Ingredients {
        identity: identity_for(state, agent),
        base,
        learned,
        skills: state.skills.brief_section(&agent.id),
        mail,
    })
}

async fn start_agent_with_brief(state: &AppState, agent: &Agent, base: &str) -> Result<(), ApiError> {
    let worktree = state
        .repos
        .worktrees()
        .into_iter()
        .find(|entry| {
            entry.worktree.repository_id == agent.repository_id
                && entry.worktree.name == agent.worktree
        })
        .ok_or_else(|| ApiError(anyhow::anyhow!("{}'s worktree is gone", agent.name)))?
        .worktree;

    let brief = compose_brief(state, agent, base).await;
    state
        .crew
        .start(&agent.id, &worktree.path, false, Some(&brief))?;
    Ok(())
}

fn strip_ansi(raw: &[u8]) -> String {
    let text = String::from_utf8_lossy(raw);
    let mut out = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();

    while let Some(character) = characters.next() {
        if character != '\u{1b}' {
            out.push(character);
            continue;
        }

        match characters.next() {
            Some('[') => {
                for inner in characters.by_ref() {
                    if inner.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            Some(']') => {
                for inner in characters.by_ref() {
                    if inner == '\u{7}' {
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    out.replace('\r', "\n")
}

fn look_at(state: &AppState, watch: &Watch, previous_frame: &str, now: u64) -> Observation {
    let session = state.manager.get(&watch.session_id);
    let alive = session.as_ref().map(|entry| entry.alive()).unwrap_or(false);
    let stats = session.as_ref().map(|entry| entry.stats());

    let idle = stats
        .map(|held| now.saturating_sub(held.last_output_at))
        .unwrap_or(0);

    let tail = state
        .manager
        .read_log(&watch.session_id, 8 * 1024)
        .map(|raw| strip_ansi(&raw))
        .unwrap_or_default();

    let quiet_turn = !tail.is_empty() && safe_to_type(previous_frame, &tail);
    let looks_done = !alive || quiet_turn || idle >= state.supervisor.rules.idle_before_finished;
    let changed_files = if looks_done {
        state
            .repos
            .review(&watch.repository_id, &watch.worktree)
            .map(|review| review.files)
            .unwrap_or(0)
    } else {
        0
    };

    Observation {
        session_alive: alive,
        quiet_turn,
        idle_seconds: idle,
        tail,
        changed_files,
        card_has_evidence: state
            .board
            .get(&watch.task_id)
            .map(|task| !task.evidence.is_empty())
            .unwrap_or(false),
        age_seconds: now.saturating_sub(watch.started_at),
    }
}

fn news_text(news: &[Watch]) -> String {
    let mut text = String::from("While you were working:");
    for watch in news {
        text.push_str(&format!(
            "\n- {} ({}) — {}",
            watch.step_id,
            watch.task_id,
            watch.reason.as_deref().unwrap_or("settled")
        ));
    }
    text.push_str("\nRead the evidence, then plan_step_done for each one you accept.");
    text
}

fn spawn_supervisor(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
        let mut last_frames: BTreeMap<String, String> = BTreeMap::new();

        loop {
            interval.tick().await;
            let now = now_secs();

            for watch in state.supervisor.working() {
                let previous = last_frames.get(&watch.session_id).cloned().unwrap_or_default();
                let seen = look_at(&state, &watch, &previous, now);

                if !watch.delivered
                    && !seen.tail.is_empty()
                    && seen.tail.to_lowercase().contains(&watch.fingerprint.to_lowercase())
                {
                    state.supervisor.mark_delivered(&watch.id);
                }

                match judge(&watch, &seen, &state.supervisor.rules) {
                    Verdict::Working => {}
                    Verdict::Resend => {
                        if safe_to_type(&previous, &seen.tail) {
                            let sent = state
                                .manager
                                .get(&watch.session_id)
                                .map(|session| {
                                    session.write_input(format!("{}\r", watch.fingerprint).as_bytes())
                                });

                            if !matches!(sent, Some(Ok(()))) {
                                continue;
                            }
                            state.supervisor.count_resend(&watch.id);
                            tracing::info!(watch = %watch.id, agent = %watch.agent_id, "the brief never landed; sent again");
                        }
                    }
                    Verdict::Finished(reason) => {
                        tracing::info!(watch = %watch.id, %reason, "a step settled");
                        let _ = state.board.attach(
                            &watch.task_id,
                            Evidence::Note {
                                text: format!("supervisor: {reason}"),
                            },
                        );
                        state.supervisor.settle(&watch.id, reason, now);
                    }
                    Verdict::LostIt(reason) => {
                        tracing::warn!(watch = %watch.id, %reason, "giving up on a step");
                        state.supervisor.give_up(&watch.id, reason, now);
                    }
                }

                last_frames.insert(watch.session_id.clone(), seen.tail);
            }

            for watch in state.supervisor.settled() {
                let previous = last_frames.get(&watch.session_id).cloned().unwrap_or_default();
                let seen = look_at(&state, &watch, &previous, now);
                last_frames.insert(watch.session_id.clone(), seen.tail.clone());

                let busy_with_new_work = state
                    .supervisor
                    .working()
                    .iter()
                    .any(|other| other.session_id == watch.session_id);

                if !should_reap(&watch, &seen, &state.supervisor.rules, busy_with_new_work, now) {
                    continue;
                }

                match state.crew.stop(&watch.agent_id) {
                    Ok(()) => {
                        state.supervisor.mark_reaped(&watch.id);
                        tracing::info!(
                            watch = %watch.id,
                            agent = %watch.agent_id,
                            "took back a pane its work had finished with"
                        );
                    }
                    Err(error) => tracing::warn!(%error, agent = %watch.agent_id, "cannot reap the pane"),
                }
            }

            if state.supervisor.wake_is_due(now) {
                let news = state.supervisor.news_for_leader();
                if news.is_empty() {
                    continue;
                }

                let leader = state
                    .crew
                    .list()
                    .into_iter()
                    .find(|agent| agent.role == "commander" && agent.session_id.is_some());

                let Some(leader) = leader else {
                    continue;
                };
                let Some(session_id) = leader.session_id.clone() else {
                    continue;
                };

                let frame = state
                    .manager
                    .read_log(&session_id, 8 * 1024)
                    .map(|raw| strip_ansi(&raw))
                    .unwrap_or_default();
                let previous = last_frames.get(&session_id).cloned().unwrap_or_default();
                last_frames.insert(session_id.clone(), frame.clone());

                if !safe_to_type(&previous, &frame) {
                    continue;
                }

                let text = news_text(&news);
                let delivered = state
                    .manager
                    .get(&session_id)
                    .map(|session| session.write_input(format!("{text}\r").as_bytes()));

                if matches!(delivered, Some(Ok(()))) {
                    let ids: Vec<String> = news.iter().map(|watch| watch.id.clone()).collect();
                    state.supervisor.leader_was_told(&ids, now);
                    tracing::info!(count = ids.len(), leader = %leader.name, "woke the commander");
                }
            }
        }
    });
}

fn spawn_routine_ticker(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));

        loop {
            interval.tick().await;
            let now = now_secs();

            for routine in state.routines.due(now) {
                let agent = state
                    .crew
                    .list()
                    .into_iter()
                    .find(|entry| entry.id == routine.agent_id);

                let outcome = match agent {
                    None => Err(format!("no agent called {}", routine.agent_id)),
                    Some(agent) => {
                        let card = state.board.create(CreateTask {
                            title: routine.name.clone(),
                            body: routine.brief.clone(),
                            repository_id: agent.repository_id.clone(),
                        });

                        match card {
                            Err(error) => Err(error.to_string()),
                            Ok(task) => {
                                let mut base = routine.brief.clone();
                                if routine.draft_only {
                                    base.push_str(
                                        "\n\nPrepare the work and stop before anything leaves this machine.",
                                    );
                                }

                                match start_agent_with_brief(&state, &agent, &base).await {
                                    Ok(()) => {
                                        let _ = state.board.record_assignment(
                                            &task.id,
                                            &agent.id,
                                            &agent.worktree,
                                            &agent.worktree,
                                        );
                                        Ok(format!("card {} handed to {}", task.id, agent.name))
                                    }
                                    Err(error) => Err(error.0.to_string()),
                                }
                            }
                        }
                    }
                };

                state.routines.record(&routine.id, now, outcome);
            }
        }
    });
}

fn is_public_asset(path: &str) -> bool {
    path == "/"
        || path == "/index.html"
        || path == "/favicon.ico"
        || path.starts_with("/assets/")
        || path.starts_with("/mobile")
}

async fn list_sessions(State(state): State<AppState>) -> Json<Vec<SessionReport>> {
    let reports = state
        .manager
        .list()
        .into_iter()
        .filter_map(|info| {
            let session = state.manager.get(&info.id)?;
            let stats = stats_with_context(&state, &info.id, session.stats());
            Some(SessionReport {
                info,
                stats,
                alive: session.alive(),
            })
        })
        .collect();
    Json(reports)
}

async fn spawn_session(
    State(state): State<AppState>,
    Json(spec): Json<PtySpawnSpec>,
) -> Result<Json<SessionInfo>, ApiError> {
    Ok(Json(state.manager.spawn(spec)?))
}

async fn spawn_generator(
    State(state): State<AppState>,
    Json(spec): Json<GeneratorSpec>,
) -> Result<Json<SessionInfo>, ApiError> {
    Ok(Json(state.manager.spawn_generator(spec)?))
}

#[derive(Deserialize)]
struct LogQuery {
    #[serde(default)]
    bytes: Option<u64>,
}

#[derive(Serialize)]
struct SessionReport {
    #[serde(flatten)]
    info: SessionInfo,
    #[serde(flatten)]
    stats: SessionStats,
    alive: bool,
}

async fn read_stats(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SessionReport>, ApiError> {
    let session = state
        .manager
        .get(&id)
        .ok_or_else(|| ApiError(anyhow::anyhow!("unknown session: {id}")))?;

    let stats = stats_with_context(&state, &id, session.stats());
    Ok(Json(SessionReport {
        info: session.info(),
        stats,
        alive: session.alive(),
    }))
}

const CONTEXT_TAIL_BYTES: u64 = 8 * 1024;

fn stats_with_context(state: &AppState, id: &str, mut stats: SessionStats) -> SessionStats {
    let Ok(tail) = state.manager.read_log(id, CONTEXT_TAIL_BYTES) else {
        return stats;
    };

    match read_context(&String::from_utf8_lossy(&tail)) {
        Some(ContextReading::PercentLeft(percent)) => stats.context_percent = Some(percent),
        Some(ContextReading::TokensUsed(tokens)) => stats.context_tokens = Some(tokens),
        None => {}
    }

    stats
}

async fn read_log(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<LogQuery>,
) -> Result<String, ApiError> {
    let bytes = query.bytes.unwrap_or(256 * 1024);
    let data = state.manager.read_log(&id, bytes)?;
    Ok(String::from_utf8_lossy(&data).into_owned())
}

#[derive(Deserialize)]
struct AddRepoBody {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    into: Option<String>,
}

#[derive(Deserialize)]
struct CreateWorktreeBody {
    name: String,
}

#[derive(Deserialize)]
struct RemoveQuery {
    #[serde(default)]
    force: bool,
}

async fn list_repos(State(state): State<AppState>) -> Json<Vec<Repository>> {
    Json(state.repos.repositories())
}

async fn add_repo(
    State(state): State<AppState>,
    Json(body): Json<AddRepoBody>,
) -> Result<Json<Repository>, ApiError> {
    if let Some(url) = body.url {
        let into = body
            .into
            .map(PathBuf::from)
            .unwrap_or_else(|| state.config.data_dir.join("clones"));
        return Ok(Json(state.repos.clone_repository(&url, &into)?));
    }

    let path = body
        .path
        .ok_or_else(|| ApiError(anyhow::anyhow!("path or url is required")))?;
    Ok(Json(state.repos.register(&PathBuf::from(path))?))
}

async fn list_worktrees(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<Vec<WorktreeStatus>> {
    let entries = state
        .repos
        .worktrees()
        .into_iter()
        .filter(|entry| entry.worktree.repository_id == id)
        .collect();
    Json(entries)
}

async fn create_worktree(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<CreateWorktreeBody>,
) -> Result<Json<Worktree>, ApiError> {
    Ok(Json(state.repos.create_worktree(&id, &body.name)?))
}

async fn remove_worktree(
    State(state): State<AppState>,
    Path((id, name)): Path<(String, String)>,
    Query(query): Query<RemoveQuery>,
) -> Result<StatusCode, ApiError> {
    state.repos.remove_worktree(&id, &name, query.force)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct StartAgentQuery {
    #[serde(default)]
    resume: bool,
}

#[derive(Deserialize)]
struct AssignBody {
    agent_id: String,
}

async fn list_tasks(State(state): State<AppState>) -> Json<Vec<Task>> {
    Json(state.board.list())
}

async fn create_task(
    State(state): State<AppState>,
    Json(request): Json<CreateTask>,
) -> Result<Json<Task>, ApiError> {
    Ok(Json(state.board.create(request)?))
}

async fn move_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<MoveTask>,
) -> Result<Json<Task>, ApiError> {
    Ok(Json(state.board.move_to(&id, body.column)?))
}

async fn delete_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.board.delete(&id)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn assign_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<AssignBody>,
) -> Result<Json<Task>, ApiError> {
    let task = state
        .board
        .get(&id)
        .ok_or_else(|| ApiError(anyhow::anyhow!("unknown task: {id}")))?;

    let agent = state
        .crew
        .list()
        .into_iter()
        .find(|entry| entry.id == body.agent_id)
        .ok_or_else(|| ApiError(anyhow::anyhow!("unknown agent: {}", body.agent_id)))?;

    if agent.repository_id != task.repository_id {
        return Err(ApiError(anyhow::anyhow!(
            "{} works in {}, not {}",
            agent.name,
            agent.repository_id,
            task.repository_id
        )));
    }

    let worktree = state
        .repos
        .worktrees()
        .into_iter()
        .find(|entry| {
            entry.worktree.repository_id == agent.repository_id
                && entry.worktree.name == agent.worktree
        })
        .ok_or_else(|| ApiError(anyhow::anyhow!("the agent's worktree is gone")))?
        .worktree;

    let brief = compose_brief(&state, &agent, &format!("{}\n\n{}", task.title, task.body)).await;
    state
        .crew
        .start(&agent.id, &worktree.path, false, Some(&brief))?;

    let updated = state
        .board
        .record_assignment(&id, &agent.id, &worktree.name, &worktree.branch)?;

    if let Some((plan, step)) = state.plans.plan_of_task(&id) {
        if let Some(session_id) = state.crew.list().into_iter().find(|entry| entry.id == agent.id).and_then(|entry| entry.session_id) {
            state.supervisor.watch(
                &plan.id,
                &step.id,
                &id,
                &agent.id,
                &session_id,
                &agent.repository_id,
                &agent.worktree,
                task.title.trim(),
                now_secs(),
            );
        }
    }

    Ok(Json(updated))
}

#[derive(Deserialize)]
struct PauseBody {
    paused: bool,
}

#[derive(Serialize)]
struct DispatchReport {
    state: DispatchState,
    decision: Decision,
    task: Option<Task>,
}

async fn dispatch_status(State(state): State<AppState>) -> Json<DispatchState> {
    Json(state.dispatch.snapshot())
}

async fn pause_dispatch(
    State(state): State<AppState>,
    Json(body): Json<PauseBody>,
) -> Json<DispatchState> {
    Json(state.dispatch.set_paused(body.paused))
}

async fn set_caps(
    State(state): State<AppState>,
    Json(caps): Json<crate::dispatch::Caps>,
) -> Json<DispatchState> {
    Json(state.dispatch.set_caps(caps))
}

async fn dispatch_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<DispatchReport>, ApiError> {
    let task = state
        .board
        .get(&id)
        .ok_or_else(|| ApiError(anyhow::anyhow!("unknown task: {id}")))?;

    let crew = state.crew.list();
    let decision = state.dispatch.decide(&task, &crew);

    match &decision {
        Decision::Assign { agent_id, reason } => {
            let agent = crew
                .iter()
                .find(|entry| &entry.id == agent_id)
                .cloned()
                .ok_or_else(|| ApiError(anyhow::anyhow!("chosen agent vanished")))?;

            let worktree = state
                .repos
                .worktrees()
                .into_iter()
                .find(|entry| {
                    entry.worktree.repository_id == agent.repository_id
                        && entry.worktree.name == agent.worktree
                })
                .ok_or_else(|| ApiError(anyhow::anyhow!("{}'s worktree is gone", agent.name)))?
                .worktree;

            let brief = compose_brief(&state, &agent, &format!("{}\n\n{}", task.title, task.body)).await;
            state
                .crew
                .start(&agent.id, &worktree.path, false, Some(&brief))?;

            let updated =
                state
                    .board
                    .record_assignment(&task.id, &agent.id, &worktree.name, &worktree.branch)?;
            let with_reason = state.board.attach(
                &task.id,
                Evidence::Note {
                    text: format!("X: {reason}"),
                },
            )?;

            let after = state.dispatch.record_assignment(&agent.id, &task.id, reason);

            let _ = updated;
            Ok(Json(DispatchReport {
                state: after,
                decision: decision.clone(),
                task: Some(with_reason),
            }))
        }
        Decision::Queue { reason } => {
            let snapshot = state.dispatch.enqueue(&task.id);

            let noted = state.board.attach(
                &task.id,
                Evidence::Note {
                    text: format!("X queued this: {reason}"),
                },
            )?;

            Ok(Json(DispatchReport {
                state: snapshot,
                decision: decision.clone(),
                task: Some(noted),
            }))
        }
        Decision::Refuse { .. } => Ok(Json(DispatchReport {
            state: state.dispatch.snapshot(),
            decision: decision.clone(),
            task: None,
        })),
    }
}

async fn review_worktree(
    State(state): State<AppState>,
    Path((id, name)): Path<(String, String)>,
) -> Result<Json<Review>, ApiError> {
    Ok(Json(state.repos.review(&id, &name)?))
}

#[derive(Deserialize)]
struct CommitBody {
    message: String,
}

async fn commit_worktree(
    State(state): State<AppState>,
    Path((id, name)): Path<(String, String)>,
    Json(body): Json<CommitBody>,
) -> Result<Json<Commit>, ApiError> {
    Ok(Json(state.repos.commit(&id, &name, &body.message)?))
}

async fn open_pull_request(
    State(state): State<AppState>,
    Path((id, name)): Path<(String, String)>,
    Json(body): Json<PullRequestBody>,
) -> Result<Json<PullRequest>, ApiError> {
    let request = state
        .repos
        .open_pull_request(&id, &name, &body.title, &body.body)?;

    if let Some(task_id) = body.task_id {
        let _ = state.board.attach(
            &task_id,
            Evidence::PullRequest {
                url: request.url.clone(),
            },
        );
        let _ = state.board.move_to(&task_id, Column::Review);
    }

    Ok(Json(request))
}

#[derive(Deserialize)]
struct PullRequestBody {
    title: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    task_id: Option<String>,
}

#[derive(Deserialize)]
struct UiCommandBody {
    name: String,
}

async fn queue_ui_command(
    State(state): State<AppState>,
    Json(body): Json<UiCommandBody>,
) -> StatusCode {
    state.ui_commands.lock().push(body.name);
    StatusCode::ACCEPTED
}

async fn take_ui_commands(State(state): State<AppState>) -> Json<Vec<String>> {
    let mut queue = state.ui_commands.lock();
    Json(std::mem::take(&mut queue))
}

async fn list_approvals(State(state): State<AppState>) -> Json<Vec<Approval>> {
    Json(state.approvals.list())
}

async fn request_approval(
    State(state): State<AppState>,
    Json(request): Json<RequestApproval>,
) -> Result<Json<Approval>, ApiError> {
    Ok(Json(state.approvals.request(request)?))
}

async fn answer_approval(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(answer): Json<AnswerApproval>,
) -> Result<Json<Approval>, ApiError> {
    Ok(Json(state.approvals.answer(&id, answer)?))
}

#[derive(Deserialize)]
struct PairBody {
    #[serde(default = "default_device_label")]
    label: String,
}

fn default_device_label() -> String {
    "phone".to_owned()
}

#[derive(Serialize)]
struct PairedDevice {
    id: String,
    label: String,
    token: String,
    scope: TokenScope,
}

async fn list_devices(State(state): State<AppState>) -> Json<Vec<crate::auth::TokenSummary>> {
    Json(state.tokens.list())
}

async fn pair_device(
    State(state): State<AppState>,
    Json(body): Json<PairBody>,
) -> Json<PairedDevice> {
    let issued = state.tokens.issue(body.label, TokenScope::Approve);
    Json(PairedDevice {
        id: issued.id.clone(),
        label: issued.label.clone(),
        token: issued.secret().to_owned(),
        scope: TokenScope::Approve,
    })
}

async fn revoke_device(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.tokens.revoke(&id)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_memories(State(state): State<AppState>) -> Json<Vec<Memory>> {
    Json(state.memories.list())
}

async fn propose_memory(
    State(state): State<AppState>,
    Json(request): Json<ProposeMemory>,
) -> Result<Json<Memory>, ApiError> {
    Ok(Json(state.memories.propose(request)?))
}

#[derive(Deserialize)]
struct ApproveBody {
    #[serde(default)]
    approved: bool,
}

#[derive(Deserialize)]
struct SearchQuery {
    #[serde(default)]
    q: String,
    #[serde(default)]
    scope: Option<Scope>,
    #[serde(default)]
    scope_id: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

async fn embed_text(state: &AppState, text: String) -> Option<Vec<f32>> {
    let settings = state.embedder.lock().clone();
    if settings.endpoint.as_deref().map(str::trim).unwrap_or("").is_empty() {
        return None;
    }

    tokio::task::spawn_blocking(move || crate::embed::embed(&settings, &text))
        .await
        .ok()
        .and_then(|result| match result {
            Ok(vector) => Some(vector),
            Err(error) => {
                tracing::warn!(%error, "the embedder did not answer; falling back to words");
                None
            }
        })
}

async fn search_memories(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Json<Vec<Recalled>> {
    let vector = embed_text(&state, query.q.clone()).await;

    let floor = state.embedder.lock().min_similarity;

    Json(state.memories.recall(
        query.scope.unwrap_or(Scope::Workspace),
        query.scope_id.as_deref().unwrap_or_default(),
        &query.q,
        vector.as_deref(),
        floor,
        query.limit.unwrap_or(8),
    ))
}

async fn read_embedder(State(state): State<AppState>) -> Json<EmbedderReport> {
    let settings = state.embedder.lock().clone();
    Json(tokio::task::spawn_blocking(move || crate::embed::probe(&settings)).await.unwrap())
}

async fn set_embedder(
    State(state): State<AppState>,
    Json(settings): Json<EmbedderSettings>,
) -> Json<EmbedderReport> {
    *state.embedder.lock() = settings.clone();
    crate::embed::save(&state.data_dir, &settings);

    let report = tokio::task::spawn_blocking({
        let settings = settings.clone();
        move || crate::embed::probe(&settings)
    })
    .await
    .unwrap();

    if report.reachable {
        let pending = state.memories.without_vectors();
        for memory in pending {
            if let Some(vector) = embed_text(&state, memory.text.clone()).await {
                state.memories.remember_vector(&memory.id, vector);
            }
        }
    }

    Json(report)
}

async fn approve_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ApproveBody>,
) -> Result<Json<Memory>, ApiError> {
    let memory = state.memories.approve(&id, body.approved)?;

    if memory.approved {
        if let Some(vector) = embed_text(&state, memory.text.clone()).await {
            state.memories.remember_vector(&memory.id, vector);
        }
    }

    Ok(Json(memory))
}

async fn forget_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.memories.forget(&id)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_mail(State(state): State<AppState>) -> Json<Vec<MailMessage>> {
    Json(state.mail.messages())
}

async fn send_mail(
    State(state): State<AppState>,
    Json(request): Json<SendMessage>,
) -> Result<Json<MailMessage>, ApiError> {
    Ok(Json(state.mail.send(request)?))
}

async fn mail_policy(State(state): State<AppState>) -> Json<MailPolicy> {
    Json(state.mail.policy())
}

async fn set_mail_policy(
    State(state): State<AppState>,
    Json(policy): Json<MailPolicy>,
) -> Json<MailPolicy> {
    Json(state.mail.set_policy(policy))
}

async fn list_routines(State(state): State<AppState>) -> Json<Vec<Routine>> {
    Json(state.routines.list())
}

async fn create_routine(
    State(state): State<AppState>,
    Json(request): Json<CreateRoutine>,
) -> Result<Json<Routine>, ApiError> {
    Ok(Json(state.routines.create(request)?))
}

#[derive(Deserialize)]
struct EnabledBody {
    enabled: bool,
}

async fn set_routine_enabled(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<EnabledBody>,
) -> Result<Json<Routine>, ApiError> {
    Ok(Json(state.routines.set_enabled(&id, body.enabled)?))
}

async fn delete_routine(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.routines.delete(&id)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_integrations(State(state): State<AppState>) -> Json<Vec<Integration>> {
    Json(state.gateway.list())
}

async fn connect_integration(
    State(state): State<AppState>,
    Json(request): Json<ConnectRequest>,
) -> Result<Json<Integration>, ApiError> {
    Ok(Json(state.gateway.connect(request)?))
}

async fn disconnect_integration(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.gateway.disconnect(&id)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn call_integration(
    State(state): State<AppState>,
    Json(request): Json<CallRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(state.gateway.call(request).await?))
}

async fn list_engines() -> Json<Vec<Engine>> {
    Json(crate::crew::engines())
}

const SILENCE_BEFORE_ATTENTION: u64 = 90;

#[derive(Serialize)]
struct AgentPresence {
    #[serde(flatten)]
    agent: Agent,
    presence: &'static str,
    since: u64,
    reason: String,
}

fn presence_of(state: &AppState, agent: &Agent, now: u64) -> AgentPresence {
    let waiting_on_human = state
        .approvals
        .list()
        .into_iter()
        .any(|approval| {
            approval.requested_by == agent.id
                && approval.verdict == crate::approvals::Verdict::Pending
        });

    if waiting_on_human {
        return AgentPresence {
            agent: agent.clone(),
            presence: "attention",
            since: 0,
            reason: "asked for approval".to_owned(),
        };
    }

    let session = agent
        .session_id
        .as_ref()
        .and_then(|id| state.manager.get(id));

    match session {
        Some(session) if session.alive() => {
            let stats = session.stats();
            let silence = now.saturating_sub(stats.last_output_at);

            if silence >= SILENCE_BEFORE_ATTENTION {
                AgentPresence {
                    agent: agent.clone(),
                    presence: "attention",
                    since: silence,
                    reason: "silent at a prompt".to_owned(),
                }
            } else {
                AgentPresence {
                    agent: agent.clone(),
                    presence: "working",
                    since: silence,
                    reason: "producing output".to_owned(),
                }
            }
        }
        _ => match agent.state {
            crate::crew::AgentState::Done => AgentPresence {
                agent: agent.clone(),
                presence: "done",
                since: 0,
                reason: "finished its run".to_owned(),
            },
            _ => AgentPresence {
                agent: agent.clone(),
                presence: "idle",
                since: 0,
                reason: "not started".to_owned(),
            },
        },
    }
}

async fn list_agents(State(state): State<AppState>) -> Json<Vec<AgentPresence>> {
    let now = now_secs();
    let agents = state
        .crew
        .list()
        .iter()
        .map(|agent| presence_of(&state, agent, now))
        .collect();
    Json(agents)
}

async fn hire_agent(
    State(state): State<AppState>,
    Json(request): Json<HireRequest>,
) -> Result<Json<Agent>, ApiError> {
    let known = state.repos.worktrees().into_iter().any(|entry| {
        entry.worktree.repository_id == request.repository_id
            && entry.worktree.name == request.worktree
    });
    if !known {
        return Err(ApiError(anyhow::anyhow!(
            "unknown worktree: {}/{}",
            request.repository_id,
            request.worktree
        )));
    }

    let commander = request.role == "commander";
    let agent = state.crew.hire(request)?;

    if commander {
        if let Err(error) = state.skills.install(&agent.id, "commanding-a-crew") {
            tracing::warn!(%error, "a commander was hired without its brief");
        }
    }

    Ok(Json(agent))
}

async fn start_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<StartAgentQuery>,
) -> Result<Json<Agent>, ApiError> {
    let agent = state
        .crew
        .list()
        .into_iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| ApiError(anyhow::anyhow!("unknown agent: {id}")))?;

    let worktree = state
        .repos
        .worktrees()
        .into_iter()
        .find(|entry| {
            entry.worktree.repository_id == agent.repository_id
                && entry.worktree.name == agent.worktree
        })
        .ok_or_else(|| ApiError(anyhow::anyhow!("agent worktree is gone")))?
        .worktree;

    let brief = compose_brief(&state, &agent, "").await;

    Ok(Json(state.crew.start(
        &id,
        &worktree.path,
        query.resume,
        crate::brief::spoken(&brief),
    )?))
}

async fn stop_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.crew.stop(&id)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn dismiss_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.crew.dismiss(&id)?;
    state.skills.forget_agent(&id);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct WriteSkill {
    id: String,
    manifest: String,
}

#[derive(Deserialize)]
struct InstallSkill {
    skill_id: String,
}

#[derive(Serialize)]
struct WorkspaceList {
    workspaces: Vec<Workspace>,
    active: Option<String>,
}

#[derive(Deserialize)]
struct ActivateWorkspace {
    #[serde(default)]
    id: Option<String>,
}

#[derive(Deserialize)]
struct WorkspaceRepos {
    repository_ids: Vec<String>,
}

async fn list_workspaces(State(state): State<AppState>) -> Json<WorkspaceList> {
    Json(WorkspaceList {
        workspaces: state.workspaces.list(),
        active: state.workspaces.active().map(|entry| entry.id),
    })
}

async fn create_workspace(
    State(state): State<AppState>,
    Json(body): Json<CreateWorkspace>,
) -> Result<Json<Workspace>, ApiError> {
    Ok(Json(state.workspaces.create(body)?))
}

async fn activate_workspace(
    State(state): State<AppState>,
    Json(body): Json<ActivateWorkspace>,
) -> Result<Json<Option<Workspace>>, ApiError> {
    Ok(Json(state.workspaces.activate(body.id.as_deref())?))
}

async fn set_workspace_repos(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<WorkspaceRepos>,
) -> Result<Json<Workspace>, ApiError> {
    Ok(Json(state.workspaces.set_repositories(&id, body.repository_ids)?))
}

async fn remove_workspace(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.workspaces.remove(&id)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct MarkStep {
    state: StepState,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    task_id: Option<String>,
}

#[derive(Deserialize)]
struct AbandonPlan {
    #[serde(default)]
    why: Option<String>,
}

#[derive(Serialize)]
struct ReadyStep {
    plan_id: String,
    goal: String,
    repository_id: String,
    step: crate::plans::Step,
}

async fn supervisor_status(State(state): State<AppState>) -> Json<Vec<Watch>> {
    Json(state.supervisor.list())
}

async fn list_plans(State(state): State<AppState>) -> Json<Vec<Plan>> {
    Json(state.plans.list())
}

async fn create_plan(
    State(state): State<AppState>,
    Json(draft): Json<DraftPlan>,
) -> Result<Json<Plan>, ApiError> {
    Ok(Json(state.plans.create(draft)?))
}

async fn read_plan(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Plan>, ApiError> {
    state
        .plans
        .get(&id)
        .map(Json)
        .ok_or_else(|| ApiError(anyhow::anyhow!("there is no plan called {id}")))
}

async fn mark_step(
    State(state): State<AppState>,
    Path((id, step)): Path<(String, String)>,
    Json(body): Json<MarkStep>,
) -> Result<Json<Plan>, ApiError> {
    if let Some(task_id) = body.task_id {
        state.plans.attach_task(&id, &step, &task_id)?;
    }

    Ok(Json(state.plans.mark(&id, &step, body.state, body.note)?))
}

async fn abandon_plan(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Option<Json<AbandonPlan>>,
) -> Result<Json<Plan>, ApiError> {
    let why = body
        .and_then(|Json(value)| value.why)
        .unwrap_or_else(|| "abandoned".to_owned());

    Ok(Json(state.plans.abandon(&id, &why)?))
}

async fn ready_steps(State(state): State<AppState>) -> Json<Vec<ReadyStep>> {
    let plans: BTreeMap<String, Plan> = state
        .plans
        .list()
        .into_iter()
        .map(|plan| (plan.id.clone(), plan))
        .collect();

    Json(
        state
            .plans
            .ready_everywhere()
            .into_iter()
            .filter_map(|(plan_id, step)| {
                plans.get(&plan_id).map(|plan| ReadyStep {
                    plan_id: plan_id.clone(),
                    goal: plan.goal.clone(),
                    repository_id: plan.repository_id.clone(),
                    step,
                })
            })
            .collect(),
    )
}

async fn list_skills(State(state): State<AppState>) -> Json<Vec<Skill>> {
    Json(state.skills.list())
}

async fn write_skill(
    State(state): State<AppState>,
    Json(body): Json<WriteSkill>,
) -> Result<Json<Skill>, ApiError> {
    Ok(Json(state.skills.write(&body.id, &body.manifest)?))
}

async fn remove_skill(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.skills.remove(&id)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_agent_skills(State(state): State<AppState>, Path(id): Path<String>) -> Json<Vec<Skill>> {
    Json(state.skills.installed_for(&id))
}

async fn install_skill(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<InstallSkill>,
) -> Result<Json<Vec<Skill>>, ApiError> {
    Ok(Json(state.skills.install(&id, &body.skill_id)?))
}

async fn uninstall_skill(
    State(state): State<AppState>,
    Path((id, skill_id)): Path<(String, String)>,
) -> Json<Vec<Skill>> {
    Json(state.skills.uninstall(&id, &skill_id))
}

async fn list_services(State(state): State<AppState>) -> Json<Vec<Service>> {
    Json(state.services.list())
}

async fn start_service(
    State(state): State<AppState>,
    Path((id, name)): Path<(String, String)>,
) -> Result<Json<Service>, ApiError> {
    let worktree = state
        .repos
        .worktrees()
        .into_iter()
        .find(|entry| entry.worktree.repository_id == id && entry.worktree.name == name)
        .ok_or_else(|| ApiError(anyhow::anyhow!("unknown worktree: {id}/{name}")))?
        .worktree;

    Ok(Json(state.services.start(
        &id,
        &name,
        &worktree.path,
        worktree.port,
    )?))
}

async fn stop_service(
    State(state): State<AppState>,
    Path((id, name)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    state.services.stop(&id, &name)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_ports(State(state): State<AppState>) -> Json<crate::ports::PortRegistry> {
    Json(state.repos.ports())
}

async fn record_metrics(
    State(state): State<AppState>,
    Json(sample): Json<Sample>,
) -> StatusCode {
    state.metrics.record(sample);
    StatusCode::NO_CONTENT
}

async fn read_metrics(State(state): State<AppState>) -> Json<Vec<Sample>> {
    Json(state.metrics.samples())
}

async fn kill_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.manager.remove(&id)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn write_input(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<InputBody>,
) -> Result<StatusCode, ApiError> {
    let session = state
        .manager
        .get(&id)
        .ok_or_else(|| ApiError(anyhow::anyhow!("unknown session: {id}")))?;
    session.write_input(body.data.as_bytes())?;
    Ok(StatusCode::NO_CONTENT)
}

async fn resize_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ResizeBody>,
) -> Result<StatusCode, ApiError> {
    let session = state
        .manager
        .get(&id)
        .ok_or_else(|| ApiError(anyhow::anyhow!("unknown session: {id}")))?;
    session.resize(body.cols, body.rows)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn stream_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    upgrade: WebSocketUpgrade,
) -> Response {
    let Some(session) = state.manager.get(&id) else {
        return (StatusCode::NOT_FOUND, "unknown session").into_response();
    };

    upgrade.on_upgrade(move |socket| pump(socket, session))
}

async fn pump(mut socket: WebSocket, session: Arc<crate::pty::Session>) {
    let (replay, mut receiver) = session.subscribe();

    for frame in replay {
        if socket.send(Message::Binary(frame)).await.is_err() {
            return;
        }
    }

    loop {
        match receiver.recv().await {
            Ok(frame) => {
                if socket.send(Message::Binary(frame)).await.is_err() {
                    break;
                }
            }
            Err(RecvError::Lagged(count)) => {
                let notice = format!("{{\"type\":\"dropped\",\"frames\":{count}}}");
                if socket.send(Message::Text(notice.into())).await.is_err() {
                    break;
                }
            }
            Err(RecvError::Closed) => break,
        }
    }
}

struct ApiError(anyhow::Error);

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        Self(error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorBody {
                error: self.0.to_string(),
            }),
        )
            .into_response()
    }
}
