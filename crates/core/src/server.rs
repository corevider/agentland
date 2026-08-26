use std::net::SocketAddr;
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
use crate::board::{Board, Column, CreateTask, Evidence, MoveTask, Task};
use crate::crew::{Agent, Crew, Engine, HireRequest};
use crate::dispatch::{decide, Decision, DispatchState};
use crate::gateway::{CallRequest, ConnectRequest, Gateway, Integration};
use crate::mail::{MailPolicy, Mailbox, Message as MailMessage, SendMessage};
use crate::memory::{Memory, MemoryStore, ProposeMemory, Scope};
use crate::routines::{CreateRoutine, Routine, Routines};
use crate::metrics::{MetricsStore, Sample};
use crate::repo::{PullRequest, RepoRegistry, Repository, Review, Worktree, WorktreeStatus};
use crate::services::{Service, ServiceRegistry};
use crate::pty::{PtyManager, PtySpawnSpec, SessionInfo, SessionStats};

#[derive(Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub token: String,
    pub allowed_hosts: Vec<String>,
    pub allowed_origins: Vec<String>,
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
    dispatch: Arc<parking_lot::Mutex<DispatchState>>,
    memories: Arc<MemoryStore>,
    mail: Arc<Mailbox>,
    routines: Arc<Routines>,
    gateway: Arc<Gateway>,
    approvals: Arc<Approvals>,
    tokens: Arc<TokenStore>,
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

    let state = AppState {
        manager,
        config: Arc::new(config),
        metrics: Arc::new(MetricsStore::new(PathBuf::from("bench-results.jsonl"))),
        repos: Arc::new(RepoRegistry::new(PathBuf::from("data"))),
        services: ServiceRegistry::new(manager_for_services),
        crew: {
            let crew = Crew::new(manager_for_crew, PathBuf::from("data"));
            crew.set_endpoint(port_for_crew, token_for_crew);
            crew
        },
        board: Arc::new(Board::new(PathBuf::from("data"))),
        dispatch: Arc::new(parking_lot::Mutex::new(DispatchState::default())),
        memories: Arc::new(MemoryStore::new(PathBuf::from("data"))),
        mail: Arc::new(Mailbox::new(PathBuf::from("data"))),
        routines: Arc::new(Routines::new(PathBuf::from("data"))),
        gateway: Arc::new(Gateway::new(PathBuf::from("data"))),
        approvals: Arc::new(Approvals::new(PathBuf::from("data"))),
        tokens: Arc::new(TokenStore::new(token_for_store, PathBuf::from("data"))),
    };

    spawn_routine_ticker(state.clone());

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
        .route("/devices", get(list_devices).post(pair_device))
        .route("/devices/{id}", delete(revoke_device))
        .route("/dispatch/tasks/{id}", post(dispatch_task))
        .route("/repos/{id}/worktrees/{name}/review", get(review_worktree))
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

fn compose_brief(state: &AppState, agent: &Agent, base: &str) -> String {
    let mut brief = base.trim().to_owned();

    let memories = state
        .memories
        .approved_for(Scope::Repository, &agent.repository_id);
    if !memories.is_empty() {
        brief.push_str("\n\nWhat this crew has learned:");
        for memory in memories {
            brief.push_str(&format!("\n- {}", memory.text));
        }
    }

    let inbox = state.mail.take_inbox(&agent.id);
    if !inbox.is_empty() {
        brief.push_str("\n\nMessages waiting for you:");
        for message in inbox {
            brief.push_str(&format!("\n- from {}: {}", message.from, message.text));
        }
    }

    brief
}

fn start_agent_with_brief(state: &AppState, agent: &Agent, base: &str) -> Result<(), ApiError> {
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

    let brief = compose_brief(state, agent, base);
    state
        .crew
        .start(&agent.id, &worktree.path, false, Some(&brief))?;
    Ok(())
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

                                match start_agent_with_brief(&state, &agent, &base) {
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
            Some(SessionReport {
                info,
                stats: session.stats(),
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

    Ok(Json(SessionReport {
        info: session.info(),
        stats: session.stats(),
        alive: session.alive(),
    }))
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
        let into = PathBuf::from(body.into.unwrap_or_else(|| "data/clones".to_owned()));
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

    let brief = compose_brief(&state, &agent, &format!("{}\n\n{}", task.title, task.body));
    state
        .crew
        .start(&agent.id, &worktree.path, false, Some(&brief))?;

    let updated = state
        .board
        .record_assignment(&id, &agent.id, &worktree.name, &worktree.branch)?;

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
    Json(state.dispatch.lock().clone())
}

async fn pause_dispatch(
    State(state): State<AppState>,
    Json(body): Json<PauseBody>,
) -> Json<DispatchState> {
    let mut dispatch = state.dispatch.lock();
    dispatch.paused = body.paused;
    Json(dispatch.clone())
}

async fn set_caps(
    State(state): State<AppState>,
    Json(caps): Json<crate::dispatch::Caps>,
) -> Json<DispatchState> {
    let mut dispatch = state.dispatch.lock();
    dispatch.caps = caps;
    Json(dispatch.clone())
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
    let decision = {
        let dispatch = state.dispatch.lock();
        decide(&dispatch, &task, &crew)
    };

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

            let brief = compose_brief(&state, &agent, &format!("{}\n\n{}", task.title, task.body));
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

            state.dispatch.lock().queue.retain(|entry| entry != &task.id);

            let _ = updated;
            Ok(Json(DispatchReport {
                state: state.dispatch.lock().clone(),
                decision: decision.clone(),
                task: Some(with_reason),
            }))
        }
        Decision::Queue { reason } => {
            let mut dispatch = state.dispatch.lock();
            if !dispatch.queue.contains(&task.id) {
                dispatch.queue.push_back(task.id.clone());
            }
            let snapshot = dispatch.clone();
            drop(dispatch);

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
            state: state.dispatch.lock().clone(),
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

async fn approve_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ApproveBody>,
) -> Result<Json<Memory>, ApiError> {
    Ok(Json(state.memories.approve(&id, body.approved)?))
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

    Ok(Json(state.crew.hire(request)?))
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

    Ok(Json(state.crew.start(&id, &worktree.path, query.resume, None)?))
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
    Ok(StatusCode::NO_CONTENT)
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
