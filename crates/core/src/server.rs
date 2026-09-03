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
use axum::{Extension, Json, Router};
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
use crate::memory::{Memory, MemoryStore, ProposeMemory, Recalled};
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
    pane_views: Arc<parking_lot::Mutex<BTreeMap<String, PaneView>>>,
    vault: Arc<crate::vault::Vault>,
    /// Things the commander should be told when it is safe to type at it. The
    /// supervisor's tick delivers these the same way it delivers its own news.
    leader_words: Arc<parking_lot::Mutex<Vec<String>>>,
    /// What each agent still has to be told, by agent id. An agent that asked a
    /// question and was answered has to hear the answer, or it waits forever.
    crew_words: Arc<parking_lot::Mutex<BTreeMap<String, Vec<String>>>>,
    notices: Arc<crate::notices::Notices>,
    /// What the engines last said about the account's quota, and when. Read
    /// rather than tallied: the quota is the account's, and every engine on the
    /// machine spends from it — including ones nobody here started.
    /// Keyed by which allowance it belongs to — an engine, and a login within
    /// it when somebody has said there is more than one. One global number
    /// meant a Claude account at ninety-five per cent stopped a Codex agent
    /// whose own week had not been touched.
    quota: Arc<parking_lot::Mutex<BTreeMap<String, (crate::budget::Usage, u64)>>>,
    /// What the crew has spent in the last minute, read from the engines' own
    /// transcripts. This app makes none of the requests it is throttling, so
    /// the only honest way to count them is to read what the engines wrote.
    journal: Arc<crate::journal::Journal>,
    goals: Arc<crate::goals::Goals>,
    standards: Arc<crate::standards::Standards>,
    voice: Arc<crate::voice::Voice>,
    /// Small things a person set: the transcriber command, and whatever else
    /// names a program or a preference rather than a piece of work.
    settings: Arc<parking_lot::Mutex<std::collections::BTreeMap<String, String>>>,
    /// What a person has already said one project may run without asking.
    permits: Arc<crate::permits::Permits>,
    spending: Arc<parking_lot::Mutex<BTreeMap<String, crate::meter::Window>>>,
    /// Per-minute ceilings, per allowance. They are a fact about somebody's
    /// plan, and two plans are two sets of numbers.
    ceilings: Arc<parking_lot::Mutex<BTreeMap<String, crate::meter::Ceilings>>>,
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
    // The vault is opened first: what the crew remembers lives in it, so the
    // store that decides what an agent is told reads from the same files a
    // person can open.
    let vault = Arc::new(crate::vault::Vault::open(&data_dir)?);
    let state = AppState {
        manager,
        config: Arc::new(config),
        metrics: Arc::new(MetricsStore::new(data_dir.join("bench-results.jsonl"))),
        repos: Arc::new(RepoRegistry::new(data_dir.clone())),
        services: ServiceRegistry::new(manager_for_services),
        crew: Crew::new(manager_for_crew, data_dir.clone()),
        board: Arc::new(Board::new(data_dir.clone())),
        dispatch: Arc::new(Dispatch::new(data_dir.clone())),
        memories: Arc::new(MemoryStore::new(vault.clone(), data_dir.clone())),
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
        vault: vault.clone(),
        leader_words: Arc::new(parking_lot::Mutex::new(Vec::new())),
        crew_words: Arc::new(parking_lot::Mutex::new(BTreeMap::new())),
        notices: Arc::new(crate::notices::Notices::default()),
        journal: Arc::new(crate::journal::Journal::new(data_dir.clone())),
        goals: Arc::new(crate::goals::Goals::new(data_dir.clone())),
        standards: Arc::new(crate::standards::Standards::new(data_dir.clone())),
        voice: Arc::new(crate::voice::Voice::new(data_dir.clone())),
        settings: Arc::new(parking_lot::Mutex::new(crate::db::load_state(
            &data_dir, "settings",
        ))),
        permits: Arc::new(crate::permits::Permits::new(data_dir.clone())),
        quota: Arc::new(parking_lot::Mutex::new(BTreeMap::new())),
        spending: Arc::new(parking_lot::Mutex::new(BTreeMap::new())),
        ceilings: Arc::new(parking_lot::Mutex::new(BTreeMap::new())),
        ui_commands: Arc::new(parking_lot::Mutex::new(Vec::new())),
        pane_views: Arc::new(parking_lot::Mutex::new(BTreeMap::new())),
    };

    spawn_routine_ticker(state.clone());
    // Before anything is watched, let go of what nothing will ever hear from.
    // Otherwise these settle all at once on the first tick and the commander is
    // woken with a day's worth of news about work that is long over.
    let stranded = state.supervisor.forget_the_stranded(now_secs(), 6 * 60 * 60);
    if stranded > 0 {
        tracing::info!(stranded, "let go of watches whose briefs never landed");
    }

    state.crew.set_standing(state.standards.file());

    spawn_supervisor(state.clone());
    spawn_pull_watcher(state.clone());

    // The crew gets a key of its own rather than the app's. An agent can read and
    // work with everything; it cannot reshape the crew or dismiss anyone, so the
    // commander asking for more rope has to go through the human either way.
    let for_the_crew = state.tokens.for_the_crew("the crew");
    state.crew.set_endpoint(port_for_crew, for_the_crew);
    // What somebody already agreed to, before the first pane opens.
    state.crew.set_learned(state.permits.everything());
    let _ = token_for_crew;

    // What was remembered before the vault held it moves in now, when the
    // workspaces are known and each memory can be filed under the project it
    // was actually about.
    let workspace_of: BTreeMap<String, String> = state
        .workspaces
        .list()
        .into_iter()
        .flat_map(|workspace| {
            workspace
                .repository_ids
                .into_iter()
                .map(move |repository| (repository, crate::vault::slug_for(&workspace.name)))
        })
        .collect();
    state.memories.take_in_what_was_kept_before(&workspace_of);

    // Whoever was working when the app last went down comes back on its own.
    // The endpoint has to be set first, or they would come up without the key
    // their tools authenticate with.
    tokio::spawn(bring_the_crew_back(state.clone()));

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
        .route("/repos/{id}", delete(forget_repo))
        .route("/repos/{id}/worktrees", get(list_worktrees).post(create_worktree))
        .route("/repos/{id}/worktrees/{name}", delete(remove_worktree))
        .route("/ports", get(list_ports))
        .route("/services", get(list_services))
        .route("/engines", get(list_engines))
        .route("/agents", get(list_agents).post(hire_agent))
        .route("/agents/{id}", delete(dismiss_agent).post(shape_agent))
        .route("/agents/{id}/holdings", get(read_holdings))
        .route("/agents/{id}/start", post(start_agent))
        .route("/agents/{id}/stop", post(stop_agent))
        .route("/tasks", get(list_tasks).post(create_task))
        .route("/tasks/{id}", delete(delete_task))
        .route("/tasks/{id}/move", post(move_task))
        .route("/tasks/{id}/place", post(place_task))
        .route("/tasks/{id}/project", post(take_task_to))
        .route("/tasks/{id}/assign", post(assign_task).delete(release_task))
        .route("/dispatch", get(dispatch_status))
        .route("/dispatch/pause", post(pause_dispatch))
        .route("/dispatch/caps", post(set_caps))
        .route("/memories", get(list_memories).post(propose_memory))
        .route("/memories/search", get(search_memories))
        .route("/memories/embedder", get(read_embedder).post(set_embedder))
        // A memory is addressed by its note's slug, which has slashes in it, so
        // the answer carries the slug in the body and the wildcard sits last.
        .route("/memories/answer", post(approve_memory))
        .route("/memories/{*slug}", delete(forget_memory))
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
        .route("/budget", get(read_budget).post(set_ceilings))
        .route("/journal", get(read_journal))
        .route("/goals", get(read_goals))
        .route("/standards", get(read_standards).post(set_standards))
        .route("/phone", get(phone_way_in))
        .route("/stop", post(stop_everything))
        .route("/commander", get(commander_says))
        .route("/voice", get(read_voice).post(set_transcriber))
        .route("/voice/start", post(start_listening))
        .route("/voice/stop", post(stop_listening))
        .route("/voice/heard", post(heard_elsewhere))
        .route("/voice/said", post(said_elsewhere))
        .route("/repos/{id}/goal", post(set_goal).delete(clear_goal))
        .route("/permits", get(read_permits).delete(forget_permit))
        .route("/stacks", get(list_starters))
        .route("/repos/{id}/commander", post(ignite))
        .route("/start", post(begin))
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
        .route("/notes", get(list_notes).post(write_note))
        .route("/vault", get(where_the_vault_is).post(redraw_the_maps))
        .route("/notices", get(list_notices).post(mark_notices_seen))
        .route("/notes/{*slug}", get(read_note).delete(forget_note))
        .route("/ui/commands", get(take_ui_commands).post(queue_ui_command))
        .route("/ui/windows", get(list_windows).post(set_window))
        .route("/dispatch/tasks/{id}", post(dispatch_task))
        .route("/repos/{id}/files", get(list_project_files))
        .route("/repos/{id}/file", get(read_project_file))
        .route("/repos/{id}/worktrees/{name}/review", get(review_worktree))
        .route("/repos/{id}/worktrees/{name}/commit", post(commit_worktree))
        .route("/repos/{id}/worktrees/{name}/pr", post(open_pull_request))
        .route("/repos/{id}/worktrees/{name}/merge", post(merge_worktree))
        .route("/repos/{id}/worktrees/{name}/review", post(submit_review))
        .route(
            "/repos/{id}/worktrees/{name}/service",
            post(start_service).delete(stop_service),
        )
        .with_state(state.clone());

    let app = match std::env::var("AGENTLAND_MOBILE_DIR").ok().filter(|dir| !dir.is_empty()) {
        Some(dir) => {
            tracing::info!(%dir, "serving the phone companion at /mobile");

            // A phone is given an address, not a path: typing the bare one
            // answered 404 with no content type, which a browser saves as
            // document.txt rather than showing.
            // Never from a cache. The page changes with the core, and a phone
            // holding yesterday's copy shows yesterday's behaviour while
            // somebody swears the fix did not work — measured, twice.
            let fresh = axum::routing::any_service(ServeDir::new(dir)).layer(
                axum::middleware::map_response(|mut answer: Response| async move {
                    answer.headers_mut().insert(
                        header::CACHE_CONTROL,
                        HeaderValue::from_static("no-store, must-revalidate"),
                    );
                    answer
                }),
            );

            app.nest_service("/mobile", fresh).route(
                "/",
                get(|| async { axum::response::Redirect::temporary("/mobile/") }),
            )
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

    // A second door, for the phone. Browsers refuse a camera and a microphone
    // on a plain http page, so the companion could show the crew but never
    // speak to it. The certificate is this machine's own — a phone will say it
    // does not recognise it, which is true, and what it buys is that the token
    // is not readable by everybody else on the network.
    if crate::phone::reachable(&state.config.host) {
        let hosts = state.config.allowed_hosts.clone();
        let secure_port = state.config.port + 1;
        let secure_addr: SocketAddr = format!("{}:{secure_port}", state.config.host).parse()?;
        let app_for_tls = app.clone();
        let data_dir = state.config.data_dir.clone();

        tokio::spawn(async move {
            match crate::tls::papers_for(&data_dir, &hosts) {
                Ok(papers) => {
                    match axum_server::tls_rustls::RustlsConfig::from_pem_file(
                        papers.certificate,
                        papers.key,
                    )
                    .await
                    {
                        Ok(held) => {
                            tracing::info!(%secure_addr, "the phone's door is open");
                            if let Err(error) = axum_server::bind_rustls(secure_addr, held)
                                .serve(app_for_tls.into_make_service())
                                .await
                            {
                                tracing::warn!(%error, "the phone's door closed");
                            }
                        }
                        Err(error) => tracing::warn!(%error, "cannot read this machine's papers"),
                    }
                }
                Err(error) => tracing::warn!(%error, "cannot make papers for this machine"),
            }
        });
    }

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "core listening");

    // Written only once it is actually listening, so anything that finds the
    // file and knocks gets an answer rather than a refused connection.
    crate::service::announce(
        &state.config.data_dir,
        &crate::service::Endpoint {
            host: state.config.host.clone(),
            port: state.config.port,
            token: state.config.token.clone(),
            pid: std::process::id(),
        },
    );

    let served = axum::serve(listener, app).await;

    // A core that has stopped should not still be advertising itself.
    crate::service::forget(&state.config.data_dir);
    served?;
    Ok(())
}

async fn guard(
    State(state): State<AppState>,
    Query(query): Query<TokenQuery>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    // HTTP/2 has no Host header: the address is in the request's own authority.
    // A browser reaching the phone's door negotiates h2 over TLS, so reading
    // only the header saw an empty string and refused every request — measured,
    // where the same call answered over http/1.1 and was forbidden over h2.
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
        .or_else(|| request.uri().authority().map(|held| held.to_string()))
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

    // Handlers need to know who is asking: the same call means one thing from the
    // app the human is looking at and another from an agent.
    let mut request = request;
    request.extensions_mut().insert(scope);

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

/// The folder name a workspace uses in the vault.
///
/// Its name, not its id: `demos/svc-demo/…` is a path a person can read in
/// Obsidian, and `ws2/svc-demo/…` is one they have to look up.
fn active_workspace_folder(state: &AppState) -> Option<String> {
    state
        .workspaces
        .active()
        .map(|held| crate::vault::slug_for(&held.name))
        .filter(|folder| !folder.is_empty())
}

/// Where a written scope points, resolved against the workspaces that exist.
///
/// A scope that names a project without saying whose — `project:svc-demo`, which
/// is how an agent writes it — belongs to the workspace that actually holds that
/// project, not to whichever workspace the person happens to be standing in.
/// Filing it by where someone was standing put a note about svc-demo under a
/// workspace called "test", which is exactly the sort of thing nobody finds again.
fn scope_for(state: &AppState, written: &str) -> crate::vault::Scope {
    let project = written
        .trim()
        .strip_prefix("project:")
        .filter(|rest| !rest.contains('/'));

    let owner = project.and_then(|project| {
        state
            .workspaces
            .list()
            .into_iter()
            .find(|workspace| workspace.repository_ids.iter().any(|held| held == project))
            .map(|workspace| crate::vault::slug_for(&workspace.name))
    });

    let workspace = owner.or_else(|| active_workspace_folder(state));
    crate::vault::Scope::parse(written, workspace.as_deref())
}

const BRIEF_MEMORIES: usize = 6;

async fn compose_brief(state: &AppState, agent: &Agent, base: &str) -> String {
    let vector = embed_text(state, base.to_owned()).await;

    // An agent is told what its project remembers, and everything its workspace
    // and the crew as a whole remember above that.
    let scope = scope_for(&state, &format!("project:{}", agent.repository_id));

    let learned = state
        .memories
        .recall(
            &scope,
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

    let written = crate::brief::compose(crate::brief::Ingredients {
        identity: identity_for(state, agent),
        base,
        learned,
        skills: state.skills.brief_section(&agent.id),
        mail,
    });

    // An engine that takes a standing instruction has already been handed the
    // house rules as a file, for every turn. One that does not is told at the
    // top of its brief instead, which costs the words each time and is still
    // better than an agent that does not know how the house works.
    if crate::crew::standing_flag(&agent.engine_id).is_some() {
        written
    } else {
        crate::standards::spoken(&state.standards.read(), &written)
    }
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

    let worktree_path = state
        .repos
        .worktrees()
        .into_iter()
        .find(|entry| {
            entry.worktree.repository_id == watch.repository_id
                && entry.worktree.name == watch.worktree
        })
        .map(|entry| entry.worktree.path);

    let transcript_says = worktree_path
        .as_ref()
        .and_then(|path| crate::transcript::was_told(path, &watch.fingerprint));

    Observation {
        session_alive: alive,
        quiet_turn,
        idle_seconds: idle,
        tail,
        changed_files,
        // Records of work put there since this watch began.
        //
        // Any evidence at all counted here, and handing a card out writes a
        // note onto it — "X: Ada is the free agent with the closest role" — so
        // a card arrived already carrying "evidence" and the tick ten seconds
        // later called the step finished. Measured: three cards were marked
        // settled 35 to 42 seconds before the commit that did the work, and a
        // review card one second after it was handed over, so nobody reviewed
        // anything.
        //
        // Ruling out remarks was not enough. A card handed out a second time
        // still carried the record from the first attempt, and settled in nine
        // seconds again — measured, four seconds before that attempt's commit.
        // What counts is work recorded since this attempt started.
        card_has_evidence: state
            .board
            .get(&watch.task_id)
            .map(|task| {
                task.evidence
                    .iter()
                    .any(|entry| entry.what.is_a_record() && entry.at >= watch.started_at)
            })
            .unwrap_or(false),
        transcript_says,
        age_seconds: now.saturating_sub(watch.started_at),
    }
}

/// What the commander is told the moment a plan finishes.
///
/// The core knows a plan is finished; it does not know what the crew learned
/// doing it. That is judgement, so the core hands over the evidence and asks the
/// one whose job is judgement to write it down.
pub fn plan_finished_word(plan: &crate::plans::Plan) -> String {
    let steps: Vec<String> = plan
        .steps
        .iter()
        .map(|step| match step.note.as_deref().map(str::trim).filter(|note| !note.is_empty()) {
            Some(note) => format!("- {} ({})", step.title, note),
            None => format!("- {}", step.title),
        })
        .collect();

    format!(
        "The plan \"{}\" is finished — {} step{} done. The steps were:\n{}\n\nWrite what the crew learned into the vault now, with note_write, before anything else: the contract that held, the trap that cost time, the thing the next agent should not have to rediscover. One note, linked to what it belongs with. Do that first and then say what you wrote.",
        plan.goal.trim(),
        plan.steps.len(),
        if plan.steps.len() == 1 { "" } else { "s" },
        steps.join("\n"),
    )
}

fn news_text(news: &[Watch]) -> String {
    let mut text = String::from("While you were working:");
    for watch in news {
        // A card with no plan behind it names itself; a plan step names both.
        let what = if watch.step_id.is_empty() {
            watch.task_id.clone()
        } else {
            format!("{} ({})", watch.step_id, watch.task_id)
        };

        text.push_str(&format!(
            "\n- {what} — {}",
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
        let mut asked_before: BTreeMap<String, bool> = BTreeMap::new();
        let mut throttled: BTreeMap<String, bool> = BTreeMap::new();

        loop {
            interval.tick().await;
            let now = now_secs();
            tracing::debug!(watches = state.supervisor.working().len(), "supervisor tick");

            for watch in state.supervisor.working() {
                let previous = last_frames.get(&watch.session_id).cloned().unwrap_or_default();
                let seen = look_at(&state, &watch, &previous, now);

                let landed = match seen.transcript_says {
                    Some(told) => told,
                    None => {
                        !seen.tail.is_empty()
                            && seen.tail.to_lowercase().contains(&watch.fingerprint.to_lowercase())
                    }
                };

                if !watch.delivered && landed {
                    state.supervisor.mark_delivered(&watch.id);
                }

                // Seeing the turn run is what separates "finished" from "has
                // not started": the verdicts that read changed files lean on it.
                if !watch.worked && crate::supervisor::turn_running(&seen.tail) {
                    state.supervisor.mark_worked(&watch.id);
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
                            state.journal.write("brief.resent", "the supervisor", &watch.task_id,
                                "the brief never landed", now);
                            tracing::info!(watch = %watch.id, agent = %watch.agent_id, "the brief never landed; sent again");
                        }
                    }
                    Verdict::Finished(reason) => {
                        tracing::info!(watch = %watch.id, %reason, "a step settled");

                        // The card's last word on this turn: who stopped, why,
                        // and what the worktree looked like when they did. A
                        // card that only ever collected remarks could not say
                        // what had actually been done on it.
                        let touched = state
                            .repos
                            .review(&watch.repository_id, &watch.worktree)
                            .ok();

                        let _ = state.board.attach(
                            &watch.task_id,
                            Evidence::Finished {
                                summary: reason.clone(),
                                files: touched.as_ref().map(|held| held.files).unwrap_or_default(),
                                insertions: touched
                                    .as_ref()
                                    .map(|held| held.insertions)
                                    .unwrap_or_default(),
                                deletions: touched
                                    .as_ref()
                                    .map(|held| held.deletions)
                                    .unwrap_or_default(),
                            },
                            &watch.agent_id,
                            now,
                        );
                        state.journal.write("step.settled", &watch.agent_id, &watch.task_id, &reason, now);

                        // A card whose step is over is not being worked on. It
                        // stayed in "working" until the commander moved it, and
                        // a commander idle at a prompt does not move anything:
                        // measured, three cards sat in "working" with the work
                        // committed, the tests passing and nobody touching them.
                        //
                        // It goes to review rather than done. Something was
                        // written and nobody has looked at it yet, which is
                        // exactly what that column means — done is still a
                        // merge, or a person saying so.
                        let wrote_something = touched
                            .as_ref()
                            .map(|held| held.files > 0)
                            .unwrap_or(false);

                        let wanted = state.board.get(&watch.task_id).and_then(|task| {
                            crate::board::where_a_settled_card_goes(
                                task.column,
                                if wrote_something { 1 } else { 0 },
                            )
                        });

                        if let Some(column) = wanted {
                            if state.board.move_to(&watch.task_id, column).is_ok() {
                                note(
                                    &state,
                                    "card.for_review",
                                    "the supervisor",
                                    &watch.task_id,
                                    &reason,
                                );
                            }
                        }

                        state.supervisor.settle(&watch.id, reason, now);
                    }
                    Verdict::LostIt(reason) => {
                        tracing::warn!(watch = %watch.id, %reason, "giving up on a step");
                        state.journal.write("step.given_up", "the supervisor", &watch.task_id, &reason, now);
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

            // The last minute of spending, refilled from the engines' own
            // transcripts. Rebuilt rather than appended to: a turn is written
            // when it ends, so the same one would be counted on every tick it
            // is still inside the window.
            {
                let since = now.saturating_sub(60);
                let mut fresh: BTreeMap<String, crate::meter::Window> = BTreeMap::new();

                for agent in state.crew.list() {
                    if agent.session_id.is_none() {
                        continue;
                    }

                    let Some(entry) = state.repos.worktrees().into_iter().find(|entry| {
                        entry.worktree.repository_id == agent.repository_id
                            && entry.worktree.name == agent.worktree
                    }) else {
                        continue;
                    };

                    let window = fresh.entry(identity_of(&agent)).or_default();
                    for spend in crate::transcript::spending_since(&entry.worktree.path, since) {
                        window.record(spend);
                    }
                }

                *state.spending.lock() = fresh;
            }

            // What the panes say about themselves, once per tick: an agent that
            // finished its turn gives its slot back, and one that is mid-turn
            // takes it. Without this an idle pane counts against the engine cap
            // forever and the dispatcher queues work nobody is doing.
            for agent in state.crew.list() {
                let Some(session_id) = agent.session_id.clone() else {
                    continue;
                };

                let tail = state
                    .manager
                    .read_log(&session_id, 8 * 1024)
                    .map(|raw| strip_ansi(&raw))
                    .unwrap_or_default();

                if tail.is_empty() {
                    continue;
                }

                // What the account has left, in the engine's own words, and
                // what the crew has spent in the last minute, from the record
                // the engine keeps. Neither number is one this app could count
                // for itself.
                if let Some(usage) = crate::budget::read_usage(&tail) {
                    // Attributed to the allowance this agent spends from, not
                    // to a single global number: two subscriptions are two
                    // weeks and neither says anything about the other.
                    state.quota.lock().insert(identity_of(&agent), (usage, now));
                }

                let limit = crate::context::read_rate_limit(&tail);
                let asking = crate::supervisor::asking_the_human(&tail);
                let working = limit.is_none() && crate::supervisor::turn_running(&tail);

                if state.crew.mark_busy(&agent.id, working) {
                    tracing::debug!(agent = %agent.id, "the pane changed what the agent is doing");
                }

                // Said once, when it starts. A throttled pane redraws its
                // counter every second and a notice per tick would bury the
                // one thing worth reading.
                let was_limited = throttled.get(&agent.id).copied().unwrap_or(false);
                match (&limit, was_limited) {
                    (Some(held), false) => {
                        throttled.insert(agent.id.clone(), true);
                        let wait = held
                            .resets_in
                            .as_deref()
                            .map(|value| format!(", resets in {value}"))
                            .unwrap_or_default();

                        tracing::warn!(agent = %agent.id, "an agent is rate limited");
                        state.journal.write("engine.rate_limited", &agent.id, "", &wait, now);
                        state.notices.push(
                            crate::notices::NewNotice {
                                kind: crate::notices::Kind::Trouble,
                                text: format!("{} is rate limited{wait}", agent.name),
                                repository_id: Some(agent.repository_id.clone()),
                                agent_id: Some(agent.id.clone()),
                                // Its own screen, where the limit is written.
                                // The crew list is for hiring and shaping, and
                                // says only that the agent exists — which the
                                // notice has already said.
                                opens: Some(format!("agent:{}", agent.id)),
                                ..Default::default()
                            },
                            now,
                        );
                    }
                    (None, true) => {
                        throttled.insert(agent.id.clone(), false);
                        tracing::info!(agent = %agent.id, "the rate limit cleared");
                        state.journal.write("engine.rate_limit_cleared", &agent.id, "", "", now);
                        state.notices.push(
                            crate::notices::NewNotice {
                                kind: crate::notices::Kind::Word,
                                text: format!("{} is off the rate limit", agent.name),
                                repository_id: Some(agent.repository_id.clone()),
                                agent_id: Some(agent.id.clone()),
                                opens: Some(format!("agent:{}", agent.id)),
                                ..Default::default()
                            },
                            now,
                        );
                    }
                    _ => {}
                }

                // A commander whose pane has filled up is traded for a fresh
                // one. Everything it needs is in the core — plans, cards,
                // evidence, the vault — so what a full pane costs is money and,
                // past a point, correctness: an engine that compacts itself
                // mid-message swallows whatever it was told at that moment.
                // Only while resting, and never in the first minutes of a
                // session, or this would be a restart loop.
                if agent.role == "commander" {
                    let alive_for = state
                        .manager
                        .get(&session_id)
                        .map(|held| now.saturating_sub(held.stats().started_at))
                        .unwrap_or_default();

                    if crate::context::wants_a_fresh_session(
                        crate::context::read_context(&tail),
                        alive_for,
                        working || asking,
                    ) {
                        let worktree = state
                            .repos
                            .worktrees()
                            .into_iter()
                            .find(|held| {
                                held.worktree.repository_id == agent.repository_id
                                    && held.worktree.name == agent.worktree
                            })
                            .map(|held| held.worktree);

                        if let Some(worktree) = worktree {
                            let _ = state.crew.stop(&agent.id);
                            match state.crew.start(&agent.id, &worktree.path, false, None) {
                                Ok(started) => {
                                    tracing::info!(agent = %agent.id, "traded a full pane for a fresh one");

                                    // The new pane knows nothing: the
                                    // conversation it was traded out of is
                                    // gone. Measured — a commander was handed a
                                    // goal, its pane filled while it read, and
                                    // the fresh one sat at an empty prompt for
                                    // half an hour with the goal lost in the
                                    // pane it replaced. It is handed what it
                                    // should be doing, the same as after a
                                    // restart.
                                    hand_back_what_it_was_holding(&state, &started).await;
                                    take_the_project_back_on(
                                        &state,
                                        &started,
                                        &worktree.path,
                                        true,
                                    )
                                    .await;
                                    state.notices.push(
                                        crate::notices::NewNotice {
                                            kind: crate::notices::Kind::Word,
                                            text: format!(
                                                "{} started a fresh session — its pane was full. Its plans and notes are untouched.",
                                                agent.name
                                            ),
                                            workspace_id: None,
                                            repository_id: Some(agent.repository_id.clone()),
                                            agent_id: Some(agent.id.clone()),
                                            opens: Some(format!("agent:{}", agent.id)),
                                        },
                                        now,
                                    );
                                }
                                Err(error) => {
                                    tracing::warn!(%error, agent = %agent.id, "cannot open a fresh pane")
                                }
                            }

                            continue;
                        }
                    }
                }

                // A goal is written down, so it can be handed over again. A
                // commander idle at a prompt, with a goal standing and no sign
                // in its own transcript of ever having been told it, was told
                // and did not hear: measured after a restart, the brief typed
                // into the pane and the turn never started.
                if agent.role == "commander" && !working && !asking {
                    if let Some(goal) = state.goals.for_project(&agent.repository_id) {
                        let worktree = state
                            .repos
                            .worktrees()
                            .into_iter()
                            .find(|held| {
                                held.worktree.repository_id == agent.repository_id
                                    && held.worktree.name == agent.worktree
                            })
                            .map(|held| held.worktree.path);

                        let opening: String = goal.text.chars().take(60).collect();
                        let told = worktree
                            .as_deref()
                            .and_then(|path| crate::transcript::was_told(path, &opening))
                            .unwrap_or(false);

                        if !told {
                            if let Some(path) = worktree {
                                let repository = state
                                    .repos
                                    .repositories()
                                    .into_iter()
                                    .find(|held| held.id == agent.repository_id);

                                if let Some(repository) = repository {
                                    let brief = what_it_is_for(&repository, Some(&goal));
                                    if let Ok(HandOver::Typed) =
                                        hand_the_work_over(&state, &agent, &path, &brief).await
                                    {
                                        note(
                                            &state,
                                            "goal.handed",
                                            "the supervisor",
                                            &agent.repository_id,
                                            &agent.id,
                                        );
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                }

                // A plan the agent wrote for the step it was handed is not a
                // question for a person: it is the agent asking whether to do
                // the work it was already given. Answered here, the way the
                // role was hired to work.
                // Asked which way to resume, the summary is both what the
                // engine recommends and what this app should pay for: it is
                // what started the pane with --resume in the first place.
                if crate::supervisor::resume_is_waiting(&tail) {
                    if say_it(&state, &session_id, "1").await {
                        note(
                            &state,
                            "resume.answered",
                            "the supervisor",
                            &agent.id,
                            "resumed from the summary",
                        );
                        continue;
                    }
                }

                if crate::supervisor::plan_is_waiting(&tail) {
                    tracing::info!(agent = %agent.id, "a plan is waiting to run");
                    let answer =
                        crate::supervisor::answer_for_the_plan(agent.permissions.as_deref());

                    if say_it(&state, &session_id, answer).await {
                        note(
                            &state,
                            "plan.approved",
                            "the supervisor",
                            &agent.id,
                            "the plan it wrote for its own step",
                        );
                        continue;
                    }
                }

                // Tried on every tick the question is held, not only on the
                // tick it appeared: a prompt is drawn a line at a time, and the
                // line naming the command is not always there yet when the
                // question first is. Repeating is safe — `already_asking` and
                // the project's own list both refuse a second one.
                if asking {
                    // A question about one command is a question worth
                    // asking once, ever. The engine offers its own "don't
                    // ask again", but that answer dies with the session and
                    // nobody is at the pane to press it — so it is put in
                    // front of a person as an approval, and their yes is
                    // kept for the project.
                    // Which of the two kinds of question this is decides which
                    // kind of rule would answer it. A command rule does not
                    // answer a question about a folder, and the pane asks again
                    // — measured, after one was granted, stored and handed over.
                    let wanted = match crate::permits::what_is_asked(&tail) {
                        Some(crate::permits::Asked::Command(command)) => {
                            crate::permits::rule_for(&command).map(|rule| (command, rule))
                        }
                        Some(crate::permits::Asked::Folder(path)) => {
                            crate::permits::rule_for_folder(&path)
                                .map(|rule| (format!("anything under {path}"), rule))
                        }
                        None => None,
                    };

                    if let Some((command, rule)) = wanted {
                        {
                            let known = state
                                .permits
                                .for_project(&agent.repository_id)
                                .contains(&rule);

                            if !known
                                && !state.approvals.already_asking(&agent.repository_id, &rule)
                            {
                                let asked = state.approvals.request_allow(
                                    format!("Let {} run `{command}`?", agent.repository_id),
                                    format!(
                                        "{} stopped on it. Saying yes lets every agent in {} run \
                                         that command from now on without asking; saying no leaves \
                                         the question with {}.",
                                        agent.name, agent.repository_id, agent.name
                                    ),
                                    crate::approvals::AllowCommand {
                                        repository_id: agent.repository_id.clone(),
                                        rule: rule.clone(),
                                        agent_id: agent.id.clone(),
                                    },
                                );

                                if asked.is_ok() {
                                    state.journal.write(
                                        "permit.asked",
                                        &agent.id,
                                        &agent.repository_id,
                                        &command,
                                        now,
                                    );
                                }
                            }
                        }
                    }
                }

                // A question the engine holds open is only answerable by a
                // person, and nothing else on screen says so. It reaches the
                // bell once, when it appears — not on every tick after.
                if asking != asked_before.get(&agent.id).copied().unwrap_or(false) {
                    asked_before.insert(agent.id.clone(), asking);

                    if asking {
                        state.notices.push(
                            crate::notices::NewNotice {
                                kind: crate::notices::Kind::Waiting,
                                text: format!("{} is holding a question open for you", agent.name),
                                workspace_id: None,
                                repository_id: Some(agent.repository_id.clone()),
                                agent_id: Some(agent.id.clone()),
                                opens: Some(format!("agent:{}", agent.id)),
                            },
                            now,
                        );

                    }
                }
            }

            // Answers owed to the crew, delivered to the agent that asked.
            let owed: Vec<(String, Vec<String>)> = state
                .crew_words
                .lock()
                .iter()
                .map(|(agent_id, words)| (agent_id.clone(), words.clone()))
                .collect();

            for (agent_id, words) in owed {
                let Some(agent) = state.crew.list().into_iter().find(|held| held.id == agent_id) else {
                    state.crew_words.lock().remove(&agent_id);
                    continue;
                };
                // An agent whose pane was taken back still has to hear this.
                // Work comes back minutes after it was handed over — a check
                // goes red, a branch stops merging — and by then the supervisor
                // has usually reaped the pane that did it. Skipping those left
                // the words in the queue for the life of the process, which is
                // why nobody ever saw one arrive.
                let live = agent
                    .session_id
                    .as_ref()
                    .filter(|id| state.manager.get(id).is_some())
                    .cloned();

                let session_id = match live {
                    Some(id) => id,
                    None => {
                        // Bringing somebody back to finish what they hold is not
                        // starting new work, so it is allowed while the week is
                        // tight and refused only when it is spent.
                        if !room_for(&state, &identity_of(&agent)).may_finish_what_is_held() {
                            continue;
                        }

                        let Some(worktree) = state.repos.worktrees().into_iter().find(|entry| {
                            entry.worktree.repository_id == agent.repository_id
                                && entry.worktree.name == agent.worktree
                        }) else {
                            state.crew_words.lock().remove(&agent_id);
                            continue;
                        };

                        let text = words.join("\n\n");
                        match state.crew.start(&agent.id, &worktree.worktree.path, true, Some(&text)) {
                            Ok(started) => {
                                state.crew_words.lock().remove(&agent_id);
                                state.journal.write(
                                    "agent.recalled",
                                    "the supervisor",
                                    &agent.id,
                                    "brought back to hear what came back",
                                    now,
                                );
                                tracing::info!(agent = %agent.id, "started an agent to tell it something");
                                let _ = started;
                            }
                            Err(error) => {
                                tracing::warn!(%error, agent = %agent.id, "cannot bring an agent back");
                            }
                        }

                        continue;
                    }
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

                let text = words.join("\n\n");

                if say_it(&state, &session_id, &text).await {
                    state.crew_words.lock().remove(&agent_id);
                    tracing::info!(agent = %agent_id, "told an agent what its question was answered");
                }
            }

            let waiting_words = !state.leader_words.lock().is_empty();

            // A wake is a turn and a turn is money. When the week or the
            // minute is tight the news waits for whoever opens the pane.
            // Asked of the commander's own allowance, once there is one to ask
            // about; until then the engine it would run on.
            let room = state
                .crew
                .list()
                .into_iter()
                .find(|agent| agent.role == "commander" && agent.session_id.is_some())
                .map(|commander| room_for(&state, &identity_of(&commander)))
                .unwrap_or(crate::budget::Room::Plenty);

            if (state.supervisor.wake_is_due(now) || waiting_words) && room.may_wake_the_commander() {
                let news = state.supervisor.news_for_leader();
                if news.is_empty() && !waiting_words {
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

                // One thing at a time. A finished plan and a roundup of watches
                // are different asks, and an agent given both does the one that
                // came last — measured: it read the evidence and never wrote the
                // note. So a queued word goes alone, and the roundup waits its
                // turn on the next tick.
                let words: Vec<String> = state.leader_words.lock().drain(..).collect();
                let carrying_news = words.is_empty();

                let text = if carrying_news {
                    if news.is_empty() {
                        continue;
                    }
                    news_text(&news)
                } else {
                    words.join("\n\n")
                };
                let delivered = say_it(&state, &session_id, &text).await;

                if delivered {
                    let ids: Vec<String> = if carrying_news {
                        news.iter().map(|watch| watch.id.clone()).collect()
                    } else {
                        Vec::new()
                    };
                    state.supervisor.leader_was_told(&ids, now);
                    state.journal.write(
                        "commander.woken",
                        "the supervisor",
                        &leader.id,
                        if carrying_news { "carried the news" } else { "had something to say" },
                        now,
                    );
                    tracing::info!(
                        count = ids.len(),
                        words = words.len(),
                        leader = %leader.name,
                        "woke the commander"
                    );
                } else {
                    // Nothing was typed, so nothing was said: put the words back
                    // rather than losing what the commander was meant to hear.
                    state.leader_words.lock().extend(words);
                }
            }
        }
    });
}

/// Follow every card whose work is on a pull request.
///
/// A card used to reach `review` and stop there: the diff was open, the branch
/// was pushed, and after that nothing in the app ever looked again. Whether it
/// merged, whether a check went red, whether the base had moved under it — all
/// of that lived on a website. So the card's life after the diff was a thing a
/// person had to carry in their head, and the agent that wrote the code never
/// heard that its tests failed.
/// How much of a failing run to carry. Enough for a stack trace and the summary
/// under it; not so much that an agent is handed a log instead of a reason.
const WHAT_A_FAILURE_IS_WORTH: usize = 1800;

fn spawn_pull_watcher(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        // When each pull request was first seen, so a repository with no CI can
        // be told from one whose CI has not started.
        let mut first_seen: BTreeMap<String, u64> = BTreeMap::new();

        loop {
            interval.tick().await;
            let now = now_secs();

            let watching: Vec<Task> = state
                .board
                .list()
                .into_iter()
                .filter(|task| {
                    matches!(task.column, Column::Review | Column::Ready)
                        && task.worktree.is_some()
                })
                .collect();

            for task in watching {
                let Some(worktree) = task.worktree.clone() else {
                    continue;
                };

                let pull = match state.repos.pull_request_state(&task.repository_id, &worktree) {
                    Ok(Some(pull)) => pull,
                    // No pull request, or no forge to ask. Neither is news.
                    Ok(None) => continue,
                    Err(error) => {
                        tracing::debug!(card = %task.id, %error, "cannot read the pull request");
                        continue;
                    }
                };

                let seen_since = *first_seen.entry(task.id.clone()).or_insert(now);
                let standing =
                    crate::pulls::where_it_stands(&pull, now.saturating_sub(seen_since));
                let said = crate::pulls::in_a_line(&standing);

                // Only a change is worth writing down, and the card is where
                // that memory belongs. Holding it in the watcher meant every
                // restart re-stamped the current standing onto every card —
                // measured, as the same line twice on a probe.
                let line = format!("pull #{}: {said}", pull.number);
                let already = task
                    .evidence
                    .iter()
                    .rev()
                    .find(|entry| entry.by == "the forge")
                    .and_then(|entry| match &entry.what {
                        Evidence::Note { text } => Some(text.as_str()),
                        _ => None,
                    });

                // A standing that has not changed is not worth writing down
                // again — but it is still worth acting on. Skipping the whole
                // block meant a card dragged back into review sat there while
                // its checks were still red, which is the board lying again.
                let is_news = already != Some(line.as_str());

                if is_news {
                    let _ = state
                        .board
                        .attach(&task.id, Evidence::Note { text: line }, "the forge", now);
                    state.journal.write("pull.changed", "the forge", &task.id, &said, now);
                }

                match &standing {
                    crate::pulls::Standing::Merged => {
                        let _ = state.board.move_to(&task.id, Column::Done);
                        if is_news {
                        state.notices.push(
                            crate::notices::NewNotice {
                                kind: crate::notices::Kind::Finished,
                                text: format!("{} merged: {}", task.id, task.title.trim()),
                                repository_id: Some(task.repository_id.clone()),
                                opens: Some("board".to_owned()),
                                ..Default::default()
                            },
                            now,
                        );
                        }
                    }
                    crate::pulls::Standing::Ready => {
                        let _ = state.board.move_to(&task.id, Column::Ready);
                        if is_news {
                        state.notices.push(
                            crate::notices::NewNotice {
                                kind: crate::notices::Kind::Waiting,
                                text: format!("{} is ready to merge", task.id),
                                repository_id: Some(task.repository_id.clone()),
                                opens: Some("board".to_owned()),
                                ..Default::default()
                            },
                            now,
                        );
                        }
                    }
                    crate::pulls::Standing::Closed => {
                        let _ = state.board.move_to(&task.id, Column::Backlog);
                    }
                    trouble if trouble.goes_back_to_the_agent() => {
                        // The card goes back to the column where work happens,
                        // and whoever wrote the code is told what broke. An
                        // agent that never hears its test failed cannot fix it.
                        let _ = state.board.move_to(&task.id, Column::Working);

                        // What comes back with the card depends on what is
                        // wrong with it. A red check needs the run's own words;
                        // a conflict needs the list of files; being behind
                        // needs neither, and saying "resolve the conflicts" to
                        // somebody who has none wastes a turn while they look.
                        // The pull request's own base, not the repository's
                        // default: a pull request can target any branch, and
                        // computing the conflict against the wrong one finds
                        // none and tells the agent there is nothing to resolve.
                        let base = if pull.base.is_empty() {
                            state
                                .repos
                                .repositories()
                                .into_iter()
                                .find(|held| held.id == task.repository_id)
                                .map(|held| held.default_branch)
                                .unwrap_or_else(|| "main".to_owned())
                        } else {
                            pull.base.clone()
                        };
                        let branch = task
                            .branch
                            .clone()
                            .unwrap_or_else(|| format!("agent/{worktree}"));

                        let (evidence, telling) = match trouble {
                            crate::pulls::Standing::ChecksFailing { .. } => {
                                let excerpt = state
                                    .repos
                                    .failing_check_log(&task.repository_id, &worktree)
                                    .ok()
                                    .flatten()
                                    .map(|log| {
                                        crate::pulls::failure_excerpt(&log, WHAT_A_FAILURE_IS_WORTH)
                                    })
                                    .filter(|held| !held.is_empty());

                                let telling = format!(
                                    "Pull request #{} for {} is not mergeable: {}{}\n\nFix it on {worktree} \
                                     and push; the card is back in working.",
                                    pull.number,
                                    task.id,
                                    said,
                                    excerpt
                                        .as_ref()
                                        .map(|held| format!("\n\nWhat the run said:\n{held}"))
                                        .unwrap_or_default(),
                                );

                                (
                                    excerpt.map(|held| format!("what the run said:\n{held}")),
                                    telling,
                                )
                            }
                            crate::pulls::Standing::Conflicted => {
                                let files = state
                                    .repos
                                    .conflicting_files(&task.repository_id, &worktree, &base)
                                    .unwrap_or_default();

                                let listed = if files.is_empty() {
                                    None
                                } else {
                                    Some(format!("conflicts with {base}: {}", files.join(", ")))
                                };

                                (
                                    listed,
                                    crate::pulls::conflict_brief(pull.number, &base, &branch, &files),
                                )
                            }
                            _ => (
                                None,
                                crate::pulls::behind_brief(pull.number, &base, &branch),
                            ),
                        };

                        // The excerpt and the telling are only fetched and
                        // sent when something changed; the column above is set
                        // either way.
                        if is_news {
                            if let Some(held) = evidence {
                                let _ = state.board.attach(
                                    &task.id,
                                    Evidence::Note { text: held },
                                    "the forge",
                                    now,
                                );
                            }

                            if let Some(who) = task.assignee.clone() {
                                state.crew_words.lock().entry(who).or_default().push(telling);
                            }
                        }

                        state.notices.push(
                            crate::notices::NewNotice {
                                kind: crate::notices::Kind::Trouble,
                                text: format!("{}: {said}", task.id),
                                repository_id: Some(task.repository_id.clone()),
                                agent_id: task.assignee.clone(),
                                opens: Some("board".to_owned()),
                                ..Default::default()
                            },
                            now,
                        );
                    }
                    _ => {}
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
                            // A routine runs where its agent lives.
                            worktree: Some(agent.worktree.clone()),
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
    /// Say yes to starting a git repository in a folder that is not one yet.
    /// Off by default: it writes to somebody's folder.
    #[serde(default)]
    start_git: bool,
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

/// The workspace new work belongs to, making one if there is nowhere yet.
///
/// Every path that opens a project goes through here. A project that joined no
/// workspace was not a small thing: the rail said the workspace held nothing
/// while the project sat in the list below it, and what its crew learned was
/// filed under a vault folder called "workspace" that nobody would think to
/// open. There is always somewhere to stand, and this is what makes it true.
fn standing_in(state: &AppState, called: &str) -> Result<(Workspace, bool), ApiError> {
    if let Some(held) = state.workspaces.active() {
        return Ok((held, false));
    }

    let made = state.workspaces.create(CreateWorkspace {
        name: called.to_owned(),
        repository_ids: Vec::new(),
    })?;

    Ok((made, true))
}

/// How much room the week has, as the panes last reported it.
///
/// Unknown is treated as room. An app that refuses to work because no engine has
/// spoken yet is an app that never starts, and the first pane to open answers
/// the question within a tick.
/// Write down what just happened.
fn note(state: &AppState, kind: &str, actor: &str, subject: &str, detail: &str) {
    state.journal.write(kind, actor, subject, detail, now_secs());
}

/// Which allowance this agent spends from.
fn identity_of(agent: &Agent) -> String {
    crate::budget::identity_of(&agent.engine_id, agent.account.as_deref())
}

/// The ceilings this allowance is held to, or the defaults nobody has changed.
fn ceilings_for(state: &AppState, identity: &str) -> crate::meter::Ceilings {
    state
        .ceilings
        .lock()
        .get(identity)
        .copied()
        .unwrap_or_default()
}

/// How much room one allowance has left.
///
/// Unknown is treated as room. An app that refuses to work because no engine on
/// that account has spoken yet is an app that never starts, and the first pane
/// to open answers the question within a tick.
fn room_for(state: &AppState, identity: &str) -> crate::budget::Room {
    let week = state
        .quota
        .lock()
        .get(identity)
        .map(|(usage, _)| usage.room())
        .unwrap_or(crate::budget::Room::Plenty);

    // Two different walls stop the same work: the week's allowance, and the
    // per-minute ceiling. Whichever is tighter is the one that decides.
    let ceilings = ceilings_for(state, identity);
    let minute = state
        .spending
        .lock()
        .get(identity)
        .map(|window| window.in_the_last_minute(now_secs()).room(&ceilings))
        .unwrap_or(crate::budget::Room::Plenty);

    crate::meter::tighter(week, minute)
}

/// The room an engine has when nobody has named a particular login on it.
fn room_for_engine(state: &AppState, engine: &str) -> crate::budget::Room {
    let wanted = crate::budget::identity_of(engine, None);

    // Every login on this engine, since a hire has not chosen one yet. The
    // tightest decides: hiring onto an exhausted login is hiring nobody.
    let identities: Vec<String> = state
        .quota
        .lock()
        .keys()
        .filter(|held| **held == wanted || held.starts_with(&format!("{engine}/")))
        .cloned()
        .collect();

    if identities.is_empty() {
        return room_for(state, &wanted);
    }

    identities
        .into_iter()
        .map(|identity| room_for(state, &identity))
        .fold(crate::budget::Room::Plenty, crate::meter::tighter)
}

async fn add_repo(
    State(state): State<AppState>,
    Json(body): Json<AddRepoBody>,
) -> Result<Json<Repository>, ApiError> {
    let opened = if let Some(url) = body.url {
        let into = body
            .into
            .map(PathBuf::from)
            .unwrap_or_else(|| state.config.data_dir.join("clones"));
        state.repos.clone_repository(&url, &into)?
    } else {
        let path = body
            .path
            .ok_or_else(|| ApiError(anyhow::anyhow!("path or url is required")))?;

        if body.start_git {
            state.repos.adopt(&PathBuf::from(path))?
        } else {
            state.repos.register(&PathBuf::from(path))?
        }
    };

    // A folder opened belongs to the workspace it was opened in, and when there
    // is not one yet it gets one named after itself rather than nothing.
    let (workspace, _) = standing_in(&state, &opened.name)?;
    state.workspaces.include(&workspace.id, &opened.id)?;
    state.workspaces.activate(Some(&workspace.id))?;

    Ok(Json(opened))
}

async fn forget_repo(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.repos.forget(&id)?;
    // A workspace that still lists a project nobody tracks any more shows a
    // name that leads nowhere.
    state.workspaces.forget_repository(&id);
    Ok(StatusCode::NO_CONTENT)
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

#[derive(Deserialize)]
struct PlaceTask {
    column: crate::board::Column,
    /// The card it should sit above. Left out, it goes to the bottom.
    #[serde(default)]
    before: Option<String>,
}

/// Drop a card into a column, in a particular place among the cards there.
async fn place_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<PlaceTask>,
) -> Result<Json<Task>, ApiError> {
    Ok(Json(state.board.place(&id, body.column, body.before.as_deref())?))
}

#[derive(Deserialize)]
struct TakeTo {
    repository_id: String,
    /// The crew asking rather than the human, so the record says who moved it.
    /// It read "a person" for three cards the commander moved itself.
    #[serde(default)]
    as_the_crew: bool,
}

/// File a card against the project it is actually about.
///
/// Wanted by the commander and impossible until now: a card on the wrong
/// project could only be discarded and written again, which throws away the
/// review it carries.
async fn take_task_to(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<TakeTo>,
) -> Result<Json<Task>, ApiError> {
    if !state
        .repos
        .repositories()
        .into_iter()
        .any(|repository| repository.id == body.repository_id)
    {
        return Err(anyhow::anyhow!("there is no project called {}", body.repository_id).into());
    }

    let moved = state.board.take_to(&id, &body.repository_id, now_secs())?;
    let by = if body.as_the_crew { "the crew" } else { "a person" };
    note(&state, "card.moved", by, &id, &body.repository_id);
    Ok(Json(moved))
}

#[derive(Default, Deserialize)]
struct DeleteTaskQuery {
    /// The crew asking rather than the human: it may throw away a card that
    /// carries nothing, and nothing else.
    #[serde(default)]
    as_the_crew: bool,
}

async fn delete_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<DeleteTaskQuery>,
) -> Result<StatusCode, ApiError> {
    if query.as_the_crew {
        state.board.discard(&id)?;
    } else {
        state.board.delete(&id)?;
    }

    Ok(StatusCode::NO_CONTENT)
}

/// What happened when an agent was given its next piece of work.
enum HandOver {
    /// It was typed into the pane the agent already had.
    Typed,
    /// A pane was started for it.
    Started,
    /// The agent is mid-turn; the work has to wait.
    Busy,
}

/// Give an agent its next brief.
///
/// An agent that finished a step still has its pane, and that pane holds
/// everything it learned doing the last one. On a plan whose steps all commit to
/// one branch the next step is nearly always for the same agent — and handing it
/// out used to mean starting a second process, which the core refuses with
/// "ada is already running". Measured on the /ready plan: the commander retried
/// that refusal in a loop while the agent sat idle at its prompt with the whole
/// context in front of it. So a live pane at rest is handed the brief where it
/// stands, a live pane mid-turn is left alone, and only an agent without a pane
/// gets a new one.
async fn hand_the_work_over(
    state: &AppState,
    agent: &Agent,
    worktree_path: &std::path::Path,
    brief: &str,
) -> Result<HandOver, ApiError> {
    let live = agent
        .session_id
        .as_ref()
        .filter(|id| state.manager.get(id).is_some())
        .cloned();

    let Some(session_id) = live else {
        state
            .crew
            .start(&agent.id, worktree_path, false, Some(brief))?;
        return Ok(HandOver::Started);
    };

    let tail = state
        .manager
        .read_log(&session_id, 8 * 1024)
        .map(|raw| strip_ansi(&raw))
        .unwrap_or_default();

    if crate::supervisor::turn_running(&tail) || crate::supervisor::asking_the_human(&tail) {
        return Ok(HandOver::Busy);
    }

    if say_it(state, &session_id, brief).await {
        Ok(HandOver::Typed)
    } else {
        Ok(HandOver::Busy)
    }
}

/// Type something into a pane and see that it was actually said.
///
/// The text and the Enter go separately: a message of several lines arrives at
/// the engine as a paste — one block, held in the composer — which swallows a
/// carriage return tacked onto its end. Measured twice, once on a plan step
/// that sat unsent as "[Pasted text #1 +20 lines]", and once on a brief handed
/// to a commander that had just come back.
///
/// One Enter is not proof either. A pane brought back with `--resume` is still
/// loading when the text lands, and the carriage return falls into an engine
/// that is not reading yet, so the turn is looked for and the Enter — never the
/// text — repeated until it starts.
async fn say_it(state: &AppState, session_id: &str, text: &str) -> bool {
    let Some(session) = state.manager.get(session_id) else {
        return false;
    };

    if session.write_input(text.as_bytes()).is_err() {
        return false;
    }

    tokio::time::sleep(std::time::Duration::from_millis(250)).await;

    // What the pane looked like with the words typed but not yet sent. A turn
    // is watched for, but a short one can start and finish between two looks —
    // measured on a two-second answer that was reported as never taken — so a
    // pane that has moved on from this frame counts as having taken it.
    let composed = state
        .manager
        .read_log(session_id, 8 * 1024)
        .map(|raw| strip_ansi(&raw))
        .unwrap_or_default();

    if session.write_input(b"\r").is_err() {
        return false;
    }

    for _ in 0..ENTER_ATTEMPTS {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let frame = state
            .manager
            .read_log(session_id, 8 * 1024)
            .map(|raw| strip_ansi(&raw))
            .unwrap_or_default();

        if crate::supervisor::turn_running(&frame) || crate::supervisor::asking_the_human(&frame) {
            return true;
        }

        if frame != composed && !frame.trim_end().ends_with(text.trim_end()) {
            return true;
        }

        if session.write_input(b"\r").is_err() {
            return false;
        }
    }

    // Out of attempts with no turn in flight: the text is sitting in a composer
    // nobody submitted. Saying "told" here is how a commander came back, was
    // handed its goal, and sat at a prompt with the goal typed in front of it
    // while the journal recorded that it had been woken.
    false
}

/// How many times the Enter is repeated while waiting for the turn to start.
/// Pressing it on a prompt that is already empty does nothing, so the cost of
/// being wrong here is a keystroke.
const ENTER_ATTEMPTS: usize = 15;

/// Follow a step that has just been handed out.
///
/// Whoever hands it out, the commander has to hear when it settles. Only the
/// human path used to do this, so a step X delegated itself was never watched:
/// the agent finished, committed, and nothing told anybody — measured on the
/// /ready plan, where step one sat at "assigned" with the work already done.
/// Put the crew back the way the app found it.
///
/// A pane dies with the app: a rebuild, a crash, a laptop closing. What the
/// agent was doing is not lost — the plan, the cards and the evidence live in
/// the core — but until now every agent had to be started again by hand, one at
/// a time, and a commander that is not running plans nothing. So whoever was
/// mid-work when the app went down is started again in its own worktree, asking
/// its engine to continue where it left off when the engine knows how.
/// Give an agent back the card it was holding when the app went down.
///
/// Coming back with a pane and no work is worse than not coming back at all: the
/// board still says the card is being worked, the plan still names the agent,
/// and the watch still points at a session that no longer exists — so it can
/// never settle and the commander is never told. Measured from the outside by
/// the commander itself, which wrote down the symptom before anyone found the
/// cause: "a card in working while crew_list shows its assignee idle and
/// waiting at a prompt".
async fn hand_back_what_it_was_holding(state: &AppState, agent: &Agent) {
    let held: Vec<Task> = state
        .board
        .list()
        .into_iter()
        .filter(|task| task.assignee.as_deref() == Some(agent.id.as_str()))
        .filter(|task| matches!(task.column, crate::board::Column::Working))
        .collect();

    for task in held {
        let brief = compose_brief(
            state,
            agent,
            &format!(
                "Picking this up again after Agentland restarted — your pane is new, the work is not.\n\n{}\n\n{}",
                task.title, task.body
            ),
        )
        .await;

        let worktree = state
            .repos
            .worktrees()
            .into_iter()
            .find(|entry| {
                entry.worktree.repository_id == agent.repository_id
                    && entry.worktree.name == agent.worktree
            })
            .map(|entry| entry.worktree.path);

        let Some(path) = worktree else {
            continue;
        };

        match hand_the_work_over(state, agent, &path, &brief).await {
            Ok(HandOver::Busy) => continue,
            Ok(_) => {
                // The old watch points at a pane that is gone; this one follows
                // the session the work is actually in.
                watch_the_step(state, agent, &task.id, task.title.trim());
                tracing::info!(agent = %agent.id, task = %task.id, "handed back the card it was holding");
            }
            Err(error) => tracing::warn!(error = %error.0, agent = %agent.id, task = %task.id, "cannot hand the card back"),
        }
    }
}

/// A commander that comes back with nothing in hand is handed the project.
///
/// Everybody else is brought back to the card they were holding; a commander
/// usually holds none, so it came back to an empty prompt and sat there until
/// somebody pressed the ignition. Its job is the project itself, which does not
/// stop being true across a restart.
async fn take_the_project_back_on(
    state: &AppState,
    agent: &Agent,
    worktree: &std::path::Path,
    said_before_is_fine: bool,
) {
    if agent.role != "commander" {
        return;
    }

    // Holding a card means it was already handed something above.
    let holding = state
        .board
        .list()
        .into_iter()
        .any(|task| {
            task.assignee.as_deref() == Some(agent.id.as_str())
                && task.column != crate::board::Column::Done
        });

    if holding {
        return;
    }

    // Waking a commander is a turn and a turn is money, so the week decides
    // whether this happens now or waits for somebody to open the pane.
    if !room_for(state, &identity_of(agent)).may_wake_the_commander() {
        return;
    }

    let Some(repository) = state
        .repos
        .repositories()
        .into_iter()
        .find(|repository| repository.id == agent.repository_id)
    else {
        return;
    };

    let brief = what_it_is_for(&repository, state.goals.for_project(&repository.id).as_ref());

    // A resumed conversation carries what was already said, so handing the same
    // brief again reads as a stutter — the commander said as much: "if the
    // repetition means the brief isn't reaching you, say so". Asked of the
    // engine's own transcript rather than the pane, because a pane that has
    // just come back has not finished drawing its history and answered no to a
    // question it had already been asked five times.
    // A session traded for a fresh one has none of the conversation the old one
    // had, so having said this before is exactly why it has to be said again.
    if !said_before_is_fine {
        let opening: String = brief.chars().take(80).collect();
        if crate::transcript::was_told(worktree, &opening).unwrap_or(false) {
            return;
        }
    }

    match hand_the_work_over(state, agent, worktree, &brief).await {
        Ok(HandOver::Busy) => {}
        Ok(_) => {
            note(state, "commander.woke", &agent.id, &repository.id, "took the project back on");
        }
        Err(error) => {
            tracing::warn!(error = %error.0, agent = %agent.id, "cannot hand the project back")
        }
    }
}

async fn bring_the_crew_back(state: AppState) {
    let interrupted = state.crew.take_the_interrupted();
    if interrupted.is_empty() {
        return;
    }

    let mut back = Vec::new();
    for agent in interrupted {
        let Some(worktree) = state
            .repos
            .worktrees()
            .into_iter()
            .find(|held| {
                held.worktree.repository_id == agent.repository_id
                    && held.worktree.name == agent.worktree
            })
            .map(|held| held.worktree)
        else {
            tracing::warn!(agent = %agent.id, "cannot bring it back: its worktree is gone");
            continue;
        };

        match state.crew.start(&agent.id, &worktree.path, true, None) {
            Ok(started) => {
                back.push(agent.name.clone());
                hand_back_what_it_was_holding(&state, &started).await;
                take_the_project_back_on(&state, &started, &worktree.path, false).await;
            }
            Err(error) => tracing::warn!(%error, agent = %agent.id, "cannot bring it back"),
        }
    }

    if back.is_empty() {
        return;
    }

    tracing::info!(crew = ?back, "brought the crew back after a restart");
    state.notices.push(
        crate::notices::NewNotice {
            kind: crate::notices::Kind::Word,
            text: format!("{} came back after Agentland restarted", back.join(", ")),
            workspace_id: None,
            repository_id: None,
            agent_id: None,
            // Several agents, so there is no one pane. Terminals is where they
            // all are; the crew list is where you would hire another.
            opens: Some("panes".to_owned()),
        },
        now_secs(),
    );
}

fn watch_the_step(state: &AppState, agent: &Agent, task_id: &str, fingerprint: &str) {
    // A card that is not a plan step is still work someone is waiting on. Only
    // plan steps used to be watched, so a card the commander wrote beside its
    // plan — the fix it noticed halfway through, the extra it split out —
    // finished in silence, and the plan sat "assigned" over committed work
    // while a person nudged by hand. Whoever holds a card gets watched; the
    // plan and step are simply empty when there is no plan behind it.
    let (plan_id, step_id) = state
        .plans
        .plan_of_task(task_id)
        .map(|(plan, step)| (plan.id, step.id))
        .unwrap_or_default();

    let Some(session_id) = state
        .crew
        .list()
        .into_iter()
        .find(|held| held.id == agent.id)
        .and_then(|held| held.session_id)
    else {
        return;
    };

    state.supervisor.watch(
        &plan_id,
        &step_id,
        task_id,
        &agent.id,
        &session_id,
        &agent.repository_id,
        &agent.worktree,
        fingerprint,
        now_secs(),
    );
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

    // A card bound to a worktree is bound for a reason: the branch it commits to
    // is checked out there and nowhere else.
    if let Some(bound) = task.worktree.as_deref() {
        if bound != agent.worktree {
            return Err(ApiError(anyhow::anyhow!(
                "{} belongs in the {bound} worktree, and {} stands in {}",
                task.id,
                agent.name,
                agent.worktree
            )));
        }
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
    if matches!(
        hand_the_work_over(&state, &agent, &worktree.path, &brief).await?,
        HandOver::Busy
    ) {
        return Err(ApiError(anyhow::anyhow!(
            "{} is in the middle of a turn — wait for it, or take the card back",
            agent.name
        )));
    }

    let updated = state
        .board
        .record_assignment(&id, &agent.id, &worktree.name, &worktree.branch)?;

    watch_the_step(&state, &agent, &id, task.title.trim());

    Ok(Json(updated))
}

/// Take a card back from whoever holds it.
///
/// The agent it was handed to is left alone — it may be mid-sentence — but the
/// supervisor stops chasing that step, so a card put on the wrong agent can be
/// handed to the right one instead of being abandoned for a fresh card.
async fn release_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Task>, ApiError> {
    let released = state.board.release(&id)?;

    for watch in state.supervisor.list() {
        if watch.task_id == id && watch.state == crate::supervisor::WatchState::Working {
            state
                .supervisor
                .give_up(&watch.id, "the card was taken back".to_owned(), now_secs());
        }
    }

    Ok(Json(released))
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

#[derive(Default, Deserialize)]
struct DispatchBody {
    /// Where this work has to happen. Naming it pins the card: only an agent
    /// standing in that worktree can be handed it.
    #[serde(default)]
    worktree: Option<String>,
}

async fn dispatch_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Option<Json<DispatchBody>>,
) -> Result<Json<DispatchReport>, ApiError> {
    let wanted = body.and_then(|Json(body)| body.worktree);
    if let Some(worktree) = wanted.as_deref() {
        state.board.bind_to_worktree(&id, Some(worktree))?;
    }

    let task = state
        .board
        .get(&id)
        .ok_or_else(|| ApiError(anyhow::anyhow!("unknown task: {id}")))?;

    let crew = state.crew.list();

    // Before choosing anybody: a card handed out with no allowance left is a
    // card that stalls half-done, which costs what it already spent and buys
    // nothing.
    // Whichever allowance the candidates would spend from. A card cannot be
    // handed to somebody whose week is gone, but somebody else's week may be
    // untouched — so this asks about the engines that could take it.
    let room = crew
        .iter()
        .filter(|agent| agent.repository_id == task.repository_id)
        .map(|agent| room_for(&state, &identity_of(agent)))
        .reduce(|best, held| match (best, held) {
            (crate::budget::Room::Plenty, _) | (_, crate::budget::Room::Plenty) => {
                crate::budget::Room::Plenty
            }
            (crate::budget::Room::Tight, _) | (_, crate::budget::Room::Tight) => {
                crate::budget::Room::Tight
            }
            _ => crate::budget::Room::Spent,
        })
        .unwrap_or(crate::budget::Room::Plenty);

    if !room.may_start_work() {
        let reason = room.in_a_line().to_owned();
        note(&state, "card.held_back", "the dispatcher", &task.id, &reason);
        let snapshot = state.dispatch.enqueue(&task.id);
        let noted = state.board.attach(
            &task.id,
            Evidence::Note {
                text: format!("held back: {reason}"),
            },
            "the dispatcher",
            now_secs(),
        )?;

        return Ok(Json(DispatchReport {
            state: snapshot,
            decision: Decision::Queue { reason },
            task: Some(noted),
        }));
    }

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
            if matches!(
                hand_the_work_over(&state, &agent, &worktree.path, &brief).await?,
                HandOver::Busy
            ) {
                let reason = format!("{} is in the middle of a turn", agent.name);
                let snapshot = state.dispatch.enqueue(&task.id);
                let noted = state.board.attach(
                    &task.id,
                    Evidence::Note {
                        text: format!("X queued this: {reason}"),
                    },
                    "the dispatcher",
                    now_secs(),
                )?;

                return Ok(Json(DispatchReport {
                    state: snapshot,
                    decision: Decision::Queue { reason },
                    task: Some(noted),
                }));
            }

            let updated =
                state
                    .board
                    .record_assignment(&task.id, &agent.id, &worktree.name, &worktree.branch)?;
            let with_reason = state.board.attach(
                &task.id,
                Evidence::Note {
                    text: format!("X: {reason}"),
                },
                "the dispatcher",
                now_secs(),
            )?;

            watch_the_step(&state, &agent, &task.id, task.title.trim());
            note(&state, "card.assigned", "the dispatcher", &task.id,
                 &format!("{} — {reason}", agent.name));

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
                "the dispatcher",
                now_secs(),
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

#[derive(Deserialize)]
struct FilesQuery {
    #[serde(default)]
    path: String,
    #[serde(default)]
    worktree: Option<String>,
}

/// Which checkout a request means: the project's own folder, or one of the
/// folders an agent works in. They are different places on disk, and a panel
/// that guessed would show a person the wrong branch's files.
fn checkout_of(state: &AppState, id: &str, worktree: Option<&str>) -> Result<PathBuf, ApiError> {
    if let Some(name) = worktree {
        let held = state
            .repos
            .worktrees()
            .into_iter()
            .find(|tree| tree.worktree.repository_id == id && tree.worktree.name == name)
            .ok_or_else(|| ApiError::from(anyhow::anyhow!("unknown worktree: {id}/{name}")))?;
        return Ok(held.worktree.path);
    }

    let held = state
        .repos
        .repositories()
        .into_iter()
        .find(|repository| repository.id == id)
        .ok_or_else(|| ApiError::from(anyhow::anyhow!("unknown repository: {id}")))?;

    Ok(held.primary_path)
}

async fn list_project_files(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<FilesQuery>,
) -> Result<Json<crate::files::Listing>, ApiError> {
    let root = checkout_of(&state, &id, query.worktree.as_deref())?;
    Ok(Json(crate::files::list(&root, &query.path)?))
}

async fn read_project_file(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<FilesQuery>,
) -> Result<Json<crate::files::FileText>, ApiError> {
    let root = checkout_of(&state, &id, query.worktree.as_deref())?;
    Ok(Json(crate::files::read(&root, &query.path)?))
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

#[derive(Deserialize)]
struct MergeBody {
    /// The card this merge finishes, when there is one.
    #[serde(default)]
    task_id: Option<String>,
}

/// Merge the pull request on a branch, and finish the card with it.
///
/// A person's call, and deliberately not something an agent reaches for on its
/// own: merging puts code in front of everyone and cannot be taken back with a
/// button. An agent that thinks it is time asks with `request_approval`.
#[derive(Deserialize)]
struct ReviewBody {
    task_id: String,
    /// approve, request_changes or comment.
    verdict: String,
    #[serde(default)]
    summary: String,
    /// Who reached it. An agent's tools fill this in from its own name.
    #[serde(default)]
    by: Option<String>,
}

/// Record a review of a card's work, and say it on the pull request.
///
/// The verdict is kept here rather than on the forge because every agent pushes
/// as the same account and GitHub will not let an account approve its own pull
/// request. What goes to GitHub is a comment naming the reviewer and the
/// verdict, so the people reading the pull request see what the crew decided.
async fn submit_review(
    State(state): State<AppState>,
    Path((id, name)): Path<(String, String)>,
    Json(body): Json<ReviewBody>,
) -> Result<Json<Task>, ApiError> {
    let verdict = crate::pulls::Verdict::read(&body.verdict).ok_or_else(|| {
        ApiError(anyhow::anyhow!(
            "a verdict is approve, request_changes or comment — not {}",
            body.verdict
        ))
    })?;

    let task = state
        .board
        .get(&body.task_id)
        .ok_or_else(|| ApiError(anyhow::anyhow!("unknown task: {}", body.task_id)))?;

    let reviewer = body
        .by
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("a person")
        .to_owned();

    crate::pulls::may_review(&reviewer, task.assignee.as_deref())
        .map_err(|why| ApiError(anyhow::anyhow!(why)))?;

    let now = now_secs();
    let comment = crate::pulls::review_comment(&reviewer, verdict, &body.summary);

    // Said on the pull request when there is one. A review of work nobody has
    // pushed yet is still a review, so this does not refuse for want of a forge.
    if let Err(error) = state.repos.comment_on_pull_request(&id, &name, &comment) {
        tracing::info!(%error, card = %task.id, "the review was kept but not posted");
    }

    let updated = state.board.attach(
        &task.id,
        Evidence::Reviewed {
            verdict: verdict.word().to_owned(),
            summary: body.summary.trim().to_owned(),
        },
        &reviewer,
        now,
    )?;

    note(&state, "card.reviewed", &reviewer, &task.id, verdict.word());

    if verdict.sends_it_back() {
        let updated = state.board.move_to(&task.id, Column::Working)?;

        if let Some(who) = task.assignee.clone() {
            state.crew_words.lock().entry(who).or_default().push(format!(
                "{reviewer} reviewed {} and asked for changes: {}\n\nThe card is back in working.",
                task.id,
                if body.summary.trim().is_empty() {
                    "no reason given".to_owned()
                } else {
                    body.summary.trim().to_owned()
                }
            ));
        }

        return Ok(Json(updated));
    }

    Ok(Json(updated))
}

async fn merge_worktree(
    State(state): State<AppState>,
    Path((id, name)): Path<(String, String)>,
    body: Option<Json<MergeBody>>,
) -> Result<Json<Task>, ApiError> {
    let task_id = body
        .and_then(|Json(body)| body.task_id)
        .ok_or_else(|| ApiError(anyhow::anyhow!("say which card this merge finishes")))?;

    let said = state.repos.merge_pull_request(&id, &name)?;
    let now = now_secs();

    state.board.attach(
        &task_id,
        Evidence::Note {
            text: if said.is_empty() {
                "merged".to_owned()
            } else {
                said.lines().next().unwrap_or("merged").to_owned()
            },
        },
        "a person",
        now,
    )?;

    note(&state, "card.merged", "a person", &task_id, &format!("{id}/{name}"));

    Ok(Json(state.board.move_to(&task_id, Column::Done)?))
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
            "a person",
            now_secs(),
        );

        // The card knew its worktree and not its branch, so it opened saying
        // "branch: none yet" while a pull request sat on one.
        if let Some(worktree) = state
            .repos
            .worktrees()
            .into_iter()
            .find(|entry| entry.worktree.repository_id == id && entry.worktree.name == name)
        {
            let _ = state.board.record_branch(&task_id, &worktree.worktree.branch);
        }

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

/// How a pane is being shown.
///
/// One pty can be looked at in more than one way — as a cell in the grid or a
/// window of its own, as a terminal or as plain readable text. The pty does not
/// care; the windows have to agree, so the answer lives here rather than in
/// either of them.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct PaneView {
    #[serde(default)]
    pub holder: String,
    #[serde(default)]
    pub readable: bool,
}

#[derive(Deserialize)]
struct SetPaneView {
    session_id: String,
    #[serde(default)]
    holder: Option<String>,
    #[serde(default)]
    readable: Option<bool>,
}

async fn list_windows(State(state): State<AppState>) -> Json<BTreeMap<String, PaneView>> {
    Json(state.pane_views.lock().clone())
}

async fn set_window(
    State(state): State<AppState>,
    Json(body): Json<SetPaneView>,
) -> Json<BTreeMap<String, PaneView>> {
    let mut held = state.pane_views.lock();
    let view = held.entry(body.session_id.clone()).or_default();

    if let Some(holder) = body.holder {
        view.holder = if holder == "grid" { String::new() } else { holder };
    }
    if let Some(readable) = body.readable {
        view.readable = readable;
    }

    // A pane shown in the grid as a terminal is the default; it needs no entry.
    if view.holder.is_empty() && !view.readable {
        held.remove(&body.session_id);
    }

    Json(held.clone())
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
    let asked_by = request.requested_by.clone();
    let approval = state.approvals.request(request)?;

    // An agent that has stopped to ask is an agent that is not working: this is
    // the kind of notice that should reach the human where they are looking.
    state.notices.push(
        crate::notices::NewNotice {
            kind: crate::notices::Kind::Waiting,
            text: format!("{asked_by} is asking: {}", approval.summary),
            agent_id: Some(asked_by),
            opens: Some("approvals".to_owned()),
            ..Default::default()
        },
        now_secs(),
    );

    Ok(Json(approval))
}

async fn answer_approval(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(answer): Json<AnswerApproval>,
) -> Result<Json<Approval>, ApiError> {
    let answered = state.approvals.answer(&id, answer)?;

    // Saying yes to a command is the act too: it is written down, and the next
    // agent that starts in that project is handed it.
    if answered.verdict == crate::approvals::Verdict::Approved {
        if let Some(allow) = answered.allows.clone() {
            if state.permits.remember(&allow.repository_id, &allow.rule) {
                // The next pane in that project starts with it. The one holding
                // the question is answered separately, below.
                state.crew.set_learned(state.permits.everything());
                note(&state, "permit.granted", "a person", &allow.repository_id, &allow.rule);
            }
        }
    }

    // Saying yes to a raise is the act, not a note about it.
    if answered.verdict == crate::approvals::Verdict::Approved {
        if let Some(grant) = answered.grants.clone() {
            state.crew.shape(
                &grant.agent_id,
                crate::crew::Shaping {
                    permissions: Some(grant.to.clone()),
                    approved_raise: true,
                    ..Default::default()
                },
            )?;
            tracing::info!(agent = %grant.agent_id, from = %grant.from, to = %grant.to, "the human raised an agent");
        }
    }

    // An answer nobody hears is not an answer. The agent that asked waits at its
    // prompt until it is told, which on the /version plan meant a commander
    // sitting on a settled question while the plan stood still.
    let verdict = if answered.verdict == crate::approvals::Verdict::Approved {
        "approved"
    } else {
        "not approved"
    };
    let note = answered
        .answered_note
        .clone()
        .filter(|note| !note.trim().is_empty())
        .map(|note| format!(" — {note}"))
        .unwrap_or_default();

    state
        .crew_words
        .lock()
        .entry(answered.requested_by.clone())
        .or_default()
        .push(format!(
            "Your question \"{}\" is {verdict}{note}. Carry on from there.",
            answered.summary
        ));

    Ok(Json(answered))
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
    let scope = scope_for(&state, &request.scope);
    Ok(Json(state.memories.propose(request, &scope, now_secs())?))
}

#[derive(Deserialize)]
struct ApproveBody {
    slug: String,
    #[serde(default)]
    approved: bool,
}

#[derive(Deserialize)]
struct SearchQuery {
    #[serde(default)]
    q: String,
    /// Where to look, in the vault's words: "shared", "workspace:atolye",
    /// "project:atolye/svc-demo". A scope also sees everything above it.
    #[serde(default)]
    scope: Option<String>,
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

    let scope = scope_for(&state, query.scope.as_deref().unwrap_or_default());

    Json(state.memories.recall(
        &scope,
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
    Json(body): Json<ApproveBody>,
) -> Result<Json<crate::memory::Approved>, ApiError> {
    let answered = state.memories.approve(&body.slug, body.approved)?;

    if answered.memory.approved {
        if let Some(vector) = embed_text(&state, answered.memory.text.clone()).await {
            state.memories.remember_vector(&answered.memory.id, vector);
        }
    }

    if let Some(replaced) = answered.replaced.as_deref() {
        tracing::info!(kept = %answered.memory.id, %replaced, "a memory replaced the one it supersedes");
    }

    Ok(Json(answered))
}

async fn forget_memory(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.memories.forget(&slug)?;
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

#[derive(Serialize)]
struct AgentPresence {
    #[serde(flatten)]
    agent: Agent,
    presence: &'static str,
    since: u64,
    reason: String,
}

/// What an agent is doing, as one word.
///
/// Silence is not the signal it looks like: a modern TUI redraws its status line
/// while nobody is doing anything, so a pane at an empty prompt never goes quiet.
/// What the engine is doing is written on the pane instead.
pub fn presence_name(
    waiting_on_human: bool,
    alive: bool,
    turn_running: bool,
    finished: bool,
) -> (&'static str, &'static str) {
    if waiting_on_human {
        return ("attention", "asked for approval");
    }

    if alive {
        return if turn_running {
            ("working", "a turn is running")
        } else {
            ("waiting", "waiting at a prompt")
        };
    }

    if finished {
        ("done", "finished its run")
    } else {
        ("idle", "not started")
    }
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

            // Silence is not the signal it looks like: a modern TUI redraws its
            // status line while nobody is doing anything, so a pane at an empty
            // prompt never goes quiet. What the engine is actually doing is
            // written on the pane, and the supervisor already knows how to read
            // it — the same reading decides presence here.
            let tail = state
                .manager
                .read_log(session.info().id.as_str(), 8 * 1024)
                .map(|raw| strip_ansi(&raw))
                .unwrap_or_default();

            // Being throttled comes before everything the pane looks like it is
            // doing: a retry counter redraws exactly like a turn, so this is
            // the one state that would otherwise be reported as work.
            if let Some(limit) = crate::context::read_rate_limit(&tail) {
                return AgentPresence {
                    agent: agent.clone(),
                    presence: "attention",
                    since: silence,
                    reason: match limit.resets_in {
                        Some(wait) => format!("rate limited, resets in {wait}"),
                        None => "rate limited".to_owned(),
                    },
                };
            }

            // A question comes first: a pane stopped on its engine's picker is
            // waiting on a person, and calling that "working" hides the one
            // thing only a person can clear.
            if crate::supervisor::asking_the_human(&tail) {
                AgentPresence {
                    agent: agent.clone(),
                    presence: "attention",
                    since: silence,
                    reason: "holding a question open".to_owned(),
                }
            } else if crate::supervisor::turn_running(&tail) {
                AgentPresence {
                    agent: agent.clone(),
                    presence: "working",
                    since: silence,
                    reason: "a turn is running".to_owned(),
                }
            } else {
                AgentPresence {
                    agent: agent.clone(),
                    presence: "waiting",
                    since: silence,
                    reason: "waiting at a prompt".to_owned(),
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

/// Put someone on the crew, with whatever their role has to know.
///
/// A commander that has not been given `commanding-a-crew` plans nothing, so the
/// skill goes on at the moment of hiring rather than being something a person
/// has to remember. Every path that hires goes through here for that reason.
fn take_on(state: &AppState, request: HireRequest) -> Result<Agent, ApiError> {
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

    Ok(agent)
}

async fn hire_agent(
    State(state): State<AppState>,
    Json(request): Json<HireRequest>,
) -> Result<Json<Agent>, ApiError> {
    Ok(Json(take_on(&state, request)?))
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

#[derive(Deserialize)]
struct NoteQuery {
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Serialize)]
struct VaultReport {
    path: String,
    notes: usize,
}

#[derive(Serialize)]
struct NoticeReport {
    notices: Vec<crate::notices::Notice>,
    unseen: usize,
    loud: bool,
}

/// What the crew wants the human to know, newest first, with a count for the
/// bell and whether any of it is the kind that should not wait.
async fn list_notices(
    State(state): State<AppState>,
    Query(query): Query<NoteQuery>,
) -> Json<NoticeReport> {
    let (unseen, loud) = state.notices.unseen();

    Json(NoticeReport {
        notices: state.notices.list(query.limit.unwrap_or(40).clamp(1, 200)),
        unseen,
        loud,
    })
}

#[derive(Deserialize)]
struct SeenBody {
    #[serde(default)]
    ids: Vec<u64>,
}

async fn mark_notices_seen(
    State(state): State<AppState>,
    Json(body): Json<SeenBody>,
) -> StatusCode {
    state.notices.mark_seen(&body.ids);
    StatusCode::NO_CONTENT
}

/// Where the vault is on disk, so the human can open the same folder in whatever
/// they keep notes in.
async fn where_the_vault_is(State(state): State<AppState>) -> Json<VaultReport> {
    Json(VaultReport {
        path: state.vault.root().to_string_lossy().to_string(),
        notes: state.vault.list().len(),
    })
}

/// Redraw every index. The maps are kept current as notes are written; this is
/// for a vault someone has been editing by hand.
async fn redraw_the_maps(State(state): State<AppState>) -> Result<Json<VaultReport>, ApiError> {
    state.vault.reindex(now_secs())?;

    Ok(Json(VaultReport {
        path: state.vault.root().to_string_lossy().to_string(),
        notes: state.vault.list().len(),
    }))
}

/// The crew's notes: everything, or what answers a question.
async fn list_notes(
    State(state): State<AppState>,
    Query(query): Query<NoteQuery>,
) -> Json<Vec<crate::vault::Note>> {
    let limit = query.limit.unwrap_or(20).clamp(1, 200);

    match query.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        Some(wanted) => Json(state.vault.search(wanted, limit)),
        None => Json(state.vault.list().into_iter().take(limit).collect()),
    }
}

async fn read_note(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<crate::vault::Note>, ApiError> {
    state
        .vault
        .get(&slug)
        .map(Json)
        .ok_or_else(|| ApiError(anyhow::anyhow!("no note called {slug}")))
}

#[derive(Deserialize)]
struct NoteDraft {
    title: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    written_by: Option<String>,
    /// Where it belongs: "shared", "workspace:<id>", "project:<workspace>/<id>",
    /// or a bare repository id. Left out, it goes to the shared shelf.
    #[serde(default)]
    scope: Option<String>,
}

/// A note written by whoever is asking. Notes are records, not instructions —
/// what an agent reads from here is quoted as somebody's writing, never obeyed.
async fn write_note(
    State(state): State<AppState>,
    Json(draft): Json<NoteDraft>,
) -> Result<Json<crate::vault::Note>, ApiError> {
    let by = draft.written_by.unwrap_or_else(|| "someone".to_owned());
    let scope = scope_for(&state, draft.scope.as_deref().unwrap_or("shared"));

    let written = state.vault.write(
        &scope,
        &draft.title,
        &draft.body,
        draft.tags,
        &by,
        now_secs(),
    )?;

    // The index is a map, and a map that goes stale is worse than none.
    let _ = state.vault.reindex(now_secs());

    Ok(Json(written))
}

async fn forget_note(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.vault.forget(&slug)?;
    Ok(StatusCode::NO_CONTENT)
}

/// The commander deciding how one of its crew is set up: which model it runs on,
/// what its pane is called, and the colour it is known by.
async fn shape_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
    scope: Option<axum::Extension<crate::auth::Scope>>,
    Json(wanted): Json<crate::crew::Shaping>,
) -> Result<Json<Agent>, ApiError> {
    // The human is the one who decides how much rope the crew gets, and this is
    // their own machine: from the app a raise simply applies. From an agent it
    // does not, whatever the agent says about itself.
    let asked_by_a_human = matches!(
        scope.map(|axum::Extension(held)| held),
        Some(crate::auth::Scope::Full) | None
    );
    let raising = wanted
        .permissions
        .as_deref()
        .map(str::trim)
        .filter(|mode| !mode.is_empty())
        .and_then(|mode| {
            let agent = state.crew.list().into_iter().find(|entry| entry.id == id)?;
            let held = agent
                .permissions
                .clone()
                .unwrap_or_else(|| crate::crew::permission_for_role(&agent.role).to_owned());

            crate::crew::is_a_raise(Some(&held), mode).then(|| (held, mode.to_owned()))
        });

    // A raise is not refused into silence: the human is asked, in words they can
    // answer on a phone, and saying yes is what carries it out.
    if let Some((held, wanted_mode)) = raising.filter(|_| !asked_by_a_human) {
        let approval = state.approvals.request_grant(
            format!("Let {id} run with {wanted_mode} instead of {held}?"),
            format!(
                "The commander asked for {id} to be raised from {held} to {wanted_mode}. Approving applies it; the agent takes it the next time it starts."
            ),
            "x",
            crate::approvals::Grant {
                agent_id: id.clone(),
                from: held.clone(),
                to: wanted_mode.clone(),
            },
        )?;

        state.notices.push(
            crate::notices::NewNotice {
                kind: crate::notices::Kind::Waiting,
                text: approval.summary.clone(),
                agent_id: Some(id.clone()),
                opens: Some("approvals".to_owned()),
                ..Default::default()
            },
            now_secs(),
        );

        return Err(ApiError(anyhow::anyhow!(
            "raising {id} from {held} to {wanted_mode} needs the human — asked as {}",
            approval.id
        )));
    }

    Ok(Json(state.crew.shape(
        &id,
        crate::crew::Shaping {
            approved_raise: asked_by_a_human,
            ..wanted
        },
    )?))
}

#[derive(Serialize)]
struct HeldCard {
    id: String,
    title: String,
    column: crate::board::Column,
}

/// What an agent still has in hand.
///
/// Dismissing is not undoable and the agent is the only thing that knows where
/// its work got to, so this is read before the question is put to anybody.
#[derive(Serialize)]
struct Holdings {
    cards: Vec<HeldCard>,
    pane_running: bool,
    /// Files changed in its worktree that were never committed.
    uncommitted: usize,
    /// Commits on its branch that are not on the base branch.
    unpushed: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    worktree: Option<String>,
    /// True when there is nothing to lose, so the caller can say so plainly.
    empty_handed: bool,
}

fn holdings_of(state: &AppState, agent: &crate::crew::Agent) -> Holdings {
    let cards: Vec<HeldCard> = state
        .board
        .list()
        .into_iter()
        .filter(|task| task.assignee.as_deref() == Some(agent.id.as_str()))
        // A finished card is not work in hand. Counting it made every agent
        // that had ever completed anything look like it was mid-something.
        .filter(|task| task.column != crate::board::Column::Done)
        .map(|task| HeldCard {
            id: task.id,
            title: task.title,
            column: task.column,
        })
        .collect();

    let held = state
        .repos
        .worktrees()
        .into_iter()
        .find(|status| {
            status.worktree.repository_id == agent.repository_id
                && status.worktree.name == agent.worktree
        });

    let pane_running = agent
        .session_id
        .as_ref()
        .is_some_and(|id| state.manager.get(id).is_some());

    let uncommitted = held.as_ref().map_or(0, |status| status.dirty_files);
    let unpushed = held.as_ref().map_or(0, |status| status.ahead);

    Holdings {
        empty_handed: cards.is_empty() && !pane_running && uncommitted == 0 && unpushed == 0,
        cards,
        pane_running,
        uncommitted,
        unpushed,
        worktree: held.map(|status| status.worktree.path.to_string_lossy().into_owned()),
    }
}

/// What is in hand, in one line, or nothing at all when the hands are empty.
fn holding_says(holding: &Holdings) -> Option<String> {
    let mut says = Vec::new();

    if !holding.cards.is_empty() {
        says.push(format!("{} unfinished card(s)", holding.cards.len()));
    }
    if holding.pane_running {
        says.push("a pane still open".to_owned());
    }
    if holding.uncommitted > 0 {
        says.push(format!("{} uncommitted file(s)", holding.uncommitted));
    }
    if holding.unpushed > 0 {
        says.push(format!(
            "{} commit(s) not on the base branch",
            holding.unpushed
        ));
    }

    (!says.is_empty()).then(|| says.join(", "))
}

async fn read_holdings(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Holdings>, ApiError> {
    let agent = state
        .crew
        .list()
        .into_iter()
        .find(|held| held.id == id)
        .ok_or_else(|| anyhow::anyhow!("unknown agent: {id}"))?;

    Ok(Json(holdings_of(&state, &agent)))
}

#[derive(Deserialize)]
struct Dismissal {
    /// Set once somebody has been shown what the agent is holding and said to
    /// go ahead anyway.
    #[serde(default)]
    anyway: bool,
}

/// Let an agent go.
///
/// Cards it held go back to the board rather than pointing at somebody who is
/// no longer there — a dismissed agent left a card reading `assignee: ro` with
/// no ro to ask about it.
async fn dismiss_agent(
    State(state): State<AppState>,
    Extension(scope): Extension<TokenScope>,
    Path(id): Path<String>,
    Query(ask): Query<Dismissal>,
) -> Result<StatusCode, ApiError> {
    // Being shown what would be lost and going ahead is a person's call. The
    // crew may let go of somebody holding nothing and nothing else, so a
    // commander tidying an idle crew cannot quietly throw work away.
    let a_person_is_asking = scope != TokenScope::Agent;
    let agent = state.crew.list().into_iter().find(|held| held.id == id);

    if let Some(agent) = &agent {
        let holding = holdings_of(&state, agent);

        // The guard is here rather than only in the panel: the same call is
        // reachable from the tools the crew itself is handed.
        if let Some(says) = holding_says(&holding) {
            if !a_person_is_asking {
                return Err(anyhow::anyhow!(
                    "{id} is holding {says} — ask a person before letting it go"
                )
                .into());
            }

            if !ask.anyway {
                return Err(
                    anyhow::anyhow!("{id} is holding {says} — dismiss it anyway to let it go")
                        .into(),
                );
            }
        }

        for card in holding.cards {
            if state.board.release(&card.id).is_ok() {
                note(&state, "card.released", "a person", &card.id, &format!("{id} was dismissed"));
            }
        }
    }

    let name = agent.as_ref().map_or(id.clone(), |held| held.name.clone());
    let project = agent
        .as_ref()
        .map(|held| held.repository_id.clone())
        .unwrap_or_default();

    state.crew.dismiss(&id)?;
    state.skills.forget_agent(&id);

    // Everything else the app decides is on the record; three agents left and
    // nothing said so. It matters more now than it did: letting somebody go is
    // no longer the human's alone, so who did it is part of what happened.
    note(
        &state,
        "agent.dismissed",
        if a_person_is_asking { "a person" } else { "the crew" },
        &project,
        &name,
    );

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

const DESK: &str = "desk";

/// The worktree a project's commander sits in.
///
/// A desk of its own, not the branch the work happens on. A commander parked in
/// the worktree a step commits to is a commander standing where an implementer
/// has to be, and the branch is checked out in exactly one place — so the card
/// that names it cannot be handed to anybody.
fn desk_for(state: &AppState, repository_id: &str) -> Result<Worktree, ApiError> {
    let standing = state.repos.worktrees().into_iter().find(|entry| {
        entry.worktree.repository_id == repository_id && entry.worktree.name == DESK
    });

    match standing {
        Some(entry) => Ok(entry.worktree),
        None => Ok(state.repos.create_worktree(repository_id, DESK)?),
    }
}

/// What a commander is told when nobody has given it a goal yet.
fn what_it_is_for(repository: &Repository, goal: Option<&crate::goals::Goal>) -> String {
    match goal {
        Some(held) => format!(
            "You are commanding {}. What is being asked for, in the words of the person who \
             asked: \"{}\" Read the project and the board first, then say how you would do it \
             and what crew that needs. Hire only for work you can name. When it is done, say so \
             — it stands until somebody says otherwise, and you will be handed it again every \
             time you come back.",
            repository.name, held.text
        ),
        None => take_the_project_on(repository),
    }
}

fn take_the_project_on(repository: &Repository) -> String {
    format!(
        "You are commanding {}. Nobody has handed you a goal yet, so start by reading the project: \
         what it is, what state it is in, and what the board already holds. Then say what you would \
         do first and what crew that needs — who to hire, what each of them is for, and which steps \
         can run at once. Hire only for work you can name. Wait for a person before you start \
         anything you have not been asked for.",
        repository.name
    )
}

#[derive(Deserialize)]
struct Ignition {
    /// What to set it going on. Left out, it is told to take the project on.
    #[serde(default)]
    brief: Option<String>,
    #[serde(default)]
    engine_id: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Serialize)]
struct Ignited {
    commander: Agent,
    worktree: Worktree,
    did: Vec<String>,
}

/// Put this project's commander at its desk and set it going.
///
/// The same call whether there is nobody yet, somebody who is not started, or
/// somebody already at work: hire if missing, start if stopped, hand over the
/// brief either way. One button can mean all three because this decides which.
async fn ignite(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Option<Json<Ignition>>,
) -> Result<Json<Ignited>, ApiError> {
    let Json(body) = body.unwrap_or(Json(Ignition {
        brief: None,
        engine_id: None,
        name: None,
    }));

    let repository = state
        .repos
        .repositories()
        .into_iter()
        .find(|repository| repository.id == id)
        .ok_or_else(|| ApiError(anyhow::anyhow!("there is no project called {id}")))?;

    let mut did = Vec::new();
    let held = state
        .crew
        .list()
        .into_iter()
        .find(|agent| agent.role == "commander" && agent.repository_id == repository.id);

    // Hiring is starting work. Telling a commander that already exists is not,
    // so a person can still reach the one they have.
    let room = match &held {
        Some(commander) => room_for(&state, &identity_of(commander)),
        None => room_for_engine(&state, "claude"),
    };

    if held.is_none() && !room.may_start_work() {
        return Err(ApiError(anyhow::anyhow!(
            "not hiring anybody right now: {}",
            room.in_a_line()
        )));
    }

    let commander = match held {
        Some(commander) => commander,
        None => {
            let desk = desk_for(&state, &repository.id)?;
            if desk.name == DESK {
                did.push(format!("gave {} a desk on {}", repository.name, desk.branch));
            }

            let engine_id = match body
                .engine_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                Some(chosen) => chosen.to_owned(),
                None => crate::start::engine_for_a_commander(&crate::crew::engines()).ok_or_else(
                    || {
                        ApiError(anyhow::anyhow!(
                            "no coding agent is installed — put one on PATH and start again"
                        ))
                    },
                )?,
            };

            let hired = take_on(
                &state,
                HireRequest {
                    name: body
                        .name
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .unwrap_or("X")
                        .to_owned(),
                    role: "commander".to_owned(),
                    engine_id,
                    repository_id: repository.id.clone(),
                    worktree: desk.name.clone(),
                    model: None,
                    title: None,
                    colour: None,
                    permissions: None,
                    account: None,
                },
            )?;

            did.push(format!("hired {} to command {}", hired.name, repository.name));
            note(&state, "agent.hired", "a person", &hired.id,
                 &format!("commander of {} on {}", repository.name, hired.engine_id));
            hired
        }
    };

    let worktree = state
        .repos
        .worktrees()
        .into_iter()
        .find(|entry| {
            entry.worktree.repository_id == commander.repository_id
                && entry.worktree.name == commander.worktree
        })
        .ok_or_else(|| ApiError(anyhow::anyhow!("{}'s worktree is gone", commander.name)))?
        .worktree;

    // A brief handed to the ignition is what the person wants doing, so it is
    // written down as the project's goal rather than only typed at a pane. A
    // pane is traded for a fresh one when it fills, and whatever was only ever
    // said to it goes with it.
    let asked_for = body
        .brief
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    if let Some(text) = asked_for {
        if state.goals.set(&repository.id, text, "a person", now_secs()).is_some() {
            note(&state, "goal.set", "a person", &repository.id, text);
        }
    }

    let base = asked_for.map(str::to_owned).unwrap_or_else(|| {
        what_it_is_for(&repository, state.goals.for_project(&repository.id).as_ref())
    });

    let brief = compose_brief(&state, &commander, &base).await;
    match hand_the_work_over(&state, &commander, &worktree.path, &brief).await? {
        HandOver::Busy => {
            return Err(ApiError(anyhow::anyhow!(
                "{} is in the middle of a turn — it is already working",
                commander.name
            )))
        }
        HandOver::Started => {
            note(&state, "agent.started", "a person", &commander.id, "at its desk");
            did.push(format!("started {} at its desk", commander.name))
        }
        HandOver::Typed => {
            note(&state, "brief.delivered", "a person", &commander.id, "told it to take the project on");
            did.push(format!("told {} to take it on", commander.name))
        }
    }

    let commander = state
        .crew
        .list()
        .into_iter()
        .find(|agent| agent.id == commander.id)
        .unwrap_or(commander);

    Ok(Json(Ignited {
        commander,
        worktree,
        did,
    }))
}

#[derive(Serialize)]
struct ProjectPermits {
    repository_id: String,
    rules: Vec<String>,
    /// Panes running in that project right now. They were handed the rules when
    /// they started and hold them until they are started again, so taking one
    /// back does not reach into a pane that is already open.
    running: Vec<String>,
}

/// What has been said yes to, per project.
///
/// A grant is permanent and silent by design — that is the point of it — which
/// makes it exactly the kind of thing that has to be readable. A list nobody can
/// see is a list nobody can correct.
async fn read_permits(State(state): State<AppState>) -> Json<Vec<ProjectPermits>> {
    let crew = state.crew.list();

    Json(
        state
            .permits
            .everything()
            .into_iter()
            .map(|(repository_id, rules)| ProjectPermits {
                running: crew
                    .iter()
                    .filter(|agent| agent.repository_id == repository_id)
                    .filter(|agent| agent.session_id.is_some())
                    .map(|agent| agent.id.clone())
                    .collect(),
                repository_id,
                rules,
            })
            .collect(),
    )
}

#[derive(Deserialize)]
struct ForgetPermit {
    repository_id: String,
    rule: String,
}

/// Take a grant back.
///
/// Yes was answered once and holds forever; no had no way of being said at all
/// until this. The next pane in that project starts without the rule.
async fn forget_permit(
    State(state): State<AppState>,
    Json(body): Json<ForgetPermit>,
) -> Result<StatusCode, ApiError> {
    if !state.permits.forget(&body.repository_id, &body.rule) {
        return Err(anyhow::anyhow!(
            "{} was not granted for {}",
            body.rule,
            body.repository_id
        )
        .into());
    }

    state.crew.set_learned(state.permits.everything());
    note(&state, "permit.revoked", "a person", &body.repository_id, &body.rule);

    Ok(StatusCode::NO_CONTENT)
}

/// A recording made somewhere else — a phone, a laptop, a browser — and sent
/// here to be read back.
///
/// The machine running the crew has no microphone of its own in most setups:
/// over a remote desktop there is nothing to record, and the phone in your hand
/// has a better one anyway. What arrives is audio; what leaves is words.
async fn heard_elsewhere(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Said>, ApiError> {
    if body.is_empty() {
        return Err(anyhow::anyhow!("no audio arrived").into());
    }

    let command = transcriber_of(&state)
        .ok_or_else(|| anyhow::anyhow!("no transcriber set — Settings, then House rules' neighbour, Voice"))?;

    let kind = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("audio/webm");

    let text = crate::voice::read_back(&state.config.data_dir, &body, kind, &command)?;
    note(&state, "voice.heard", "a person", "elsewhere", &text);

    Ok(Json(Said { text }))
}

#[derive(Deserialize)]
struct SaidElsewhere {
    text: String,
    /// An agent id or name to say it to. Left out, it becomes the project's goal.
    #[serde(default)]
    to: Option<String>,
    /// The project whose goal it becomes, when it is a goal.
    #[serde(default)]
    repository_id: Option<String>,
}

/// Words from somewhere else, put where they were meant to go.
async fn said_elsewhere(
    State(state): State<AppState>,
    Json(body): Json<SaidElsewhere>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let text = body.text.trim().to_owned();
    if text.is_empty() {
        return Err(anyhow::anyhow!("nothing was said").into());
    }

    if let Some(who) = body.to.as_deref().map(str::trim).filter(|who| !who.is_empty()) {
        let crew = state.crew.list();
        let held = crew
            .iter()
            .find(|agent| agent.id.eq_ignore_ascii_case(who) || agent.name.eq_ignore_ascii_case(who))
            .ok_or_else(|| anyhow::anyhow!("no agent called {who}"))?;

        let session_id = held
            .session_id
            .clone()
            .filter(|id| state.manager.get(id).is_some())
            .ok_or_else(|| anyhow::anyhow!("{} has no pane open", held.name))?;

        if !say_it(&state, &session_id, &text).await {
            return Err(anyhow::anyhow!("{} did not take it", held.name).into());
        }

        note(&state, "voice.said", "a person", &held.id, &text);
        return Ok(Json(serde_json::json!({ "told": held.id })));
    }

    let repository_id = body
        .repository_id
        .or_else(|| state.repos.repositories().first().map(|held| held.id.clone()))
        .ok_or_else(|| anyhow::anyhow!("there is no project to give a goal to"))?;

    let goal = state
        .goals
        .set(&repository_id, &text, "a person", now_secs())
        .ok_or_else(|| anyhow::anyhow!("a goal is a paragraph, and not an empty one"))?;

    note(&state, "goal.set", "a person", &goal.repository_id, &goal.text);
    Ok(Json(serde_json::json!({ "goal": goal.repository_id })))
}

#[derive(Serialize)]
struct CommanderSays {
    id: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    presence: Option<String>,
    /// The last few things it said, with the interface thrown away.
    said: Vec<String>,
}

/// The commander's latest, for somebody holding a phone.
///
/// A person away from the machine wants to know what the crew is up to, and
/// the commander is where that is said. Its pane redraws itself several times
/// a second, so the words are picked out here rather than on the phone.
async fn commander_says(State(state): State<AppState>) -> Json<Vec<CommanderSays>> {
    let mut answers = Vec::new();

    for agent in state.crew.list() {
        if agent.role != "commander" {
            continue;
        }

        // Played into a screen the size of the pane's own, rather than
        // stripped: a pane draws by moving the cursor, so stripping the moves
        // gave the phone "RemoteControlnotstartedhere", and a screen of the
        // wrong width gives the right-hand edges of wrapped lines.
        let said = agent
            .session_id
            .as_ref()
            .and_then(|id| state.manager.get(id).map(|session| (id.clone(), session.info())))
            .and_then(|(id, info)| {
                state
                    .manager
                    .read_log(&id, 128 * 1024)
                    .ok()
                    .map(|raw| crate::chatter::on_screen(&raw, info.rows.max(24), info.cols.max(60)))
            })
            .map(|screen| crate::chatter::last_words(&screen, 8))
            .unwrap_or_default();

        answers.push(CommanderSays {
            id: agent.id.clone(),
            name: agent.name.clone(),
            presence: Some(format!("{:?}", agent.state).to_lowercase()),
            said,
        });
    }

    Json(answers)
}

#[derive(Serialize)]
struct PhoneWayIn {
    /// The addresses a phone could use, best first.
    urls: Vec<String>,
    /// The first of them, drawn as something a camera can read.
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
    /// False when the core answers only the machine it runs on, which is a
    /// code that goes nowhere.
    reachable: bool,
}

/// Asked, from the tray, to stop the crew and go.
///
/// The window is one client of a core that outlives it, which is right for a
/// closed window and wrong for somebody who chose "stop the crew and quit".
/// The announcement goes first, so nothing finds a core that is on its way
/// out; then the answer; then the process, and every pane with it.
async fn stop_everything(State(state): State<AppState>) -> StatusCode {
    crate::service::forget(&state.config.data_dir);

    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        std::process::exit(0);
    });

    StatusCode::NO_CONTENT
}

/// How to get a phone in without typing a token off a screen.
async fn phone_way_in(State(state): State<AppState>) -> Json<PhoneWayIn> {
    let port = state.config.port;
    let token = &state.config.token;
    let reachable = crate::phone::reachable(&state.config.host);

    // The secure door, because a phone that arrives there can use its camera
    // and its microphone; the plain one is offered after it for anything that
    // will not accept a certificate nobody signed.
    let urls: Vec<String> = if reachable {
        let hosts: Vec<String> = crate::service::on_this_network(port)
            .into_iter()
            .filter_map(|held| held.rsplit_once(':').map(|(host, _)| host.to_owned()))
            .collect();

        hosts
            .iter()
            .map(|host| crate::phone::url_for_securely(host, port + 1, token))
            .chain(hosts.iter().map(|host| crate::phone::url_for(host, port, token)))
            .collect()
    } else {
        Vec::new()
    };

    let code = urls.first().and_then(|url| crate::phone::as_a_code(url));

    Json(PhoneWayIn {
        urls,
        code,
        reachable,
    })
}

#[derive(Serialize)]
struct HouseRules {
    text: String,
    /// Whether they are on disk for an engine to read.
    held: bool,
}

/// How the house works, for every agent, in every project.
async fn read_standards(State(state): State<AppState>) -> Json<HouseRules> {
    Json(HouseRules {
        text: state.standards.read(),
        held: state.standards.file().is_some(),
    })
}

#[derive(Deserialize)]
struct SetStandards {
    text: String,
}

async fn set_standards(
    State(state): State<AppState>,
    Json(body): Json<SetStandards>,
) -> Result<Json<HouseRules>, ApiError> {
    state.standards.set(&body.text)?;
    state.crew.set_standing(state.standards.file());

    note(
        &state,
        "standards.set",
        "a person",
        "",
        if body.text.trim().is_empty() { "cleared" } else { "written" },
    );

    Ok(Json(HouseRules {
        text: state.standards.read(),
        held: state.standards.file().is_some(),
    }))
}

#[derive(Serialize)]
struct VoiceState {
    /// The recorder found on this machine, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    recorder: Option<&'static str>,
    /// The command that reads the words back, as somebody set it.
    #[serde(skip_serializing_if = "Option::is_none")]
    transcriber: Option<String>,
    listening: bool,
}

fn transcriber_of(state: &AppState) -> Option<String> {
    state
        .settings
        .lock()
        .get("transcriber")
        .cloned()
        .filter(|held| !held.trim().is_empty())
        .or_else(|| std::env::var("AGENTLAND_TRANSCRIBER").ok())
        .filter(|held| !held.trim().is_empty())
}

async fn read_voice(State(state): State<AppState>) -> Json<VoiceState> {
    Json(VoiceState {
        recorder: state.voice.recorder(),
        transcriber: transcriber_of(&state),
        listening: state.voice.listening(),
    })
}

#[derive(Deserialize)]
struct SetTranscriber {
    command: String,
}

/// The command that reads a recording back. A person's to set: it names a
/// program on their machine, and nothing is bundled.
async fn set_transcriber(
    State(state): State<AppState>,
    Json(body): Json<SetTranscriber>,
) -> Json<VoiceState> {
    state
        .settings
        .lock()
        .insert("transcriber".to_owned(), body.command.trim().to_owned());
    crate::db::save_state(&state.config.data_dir, "settings", &*state.settings.lock());

    Json(VoiceState {
        recorder: state.voice.recorder(),
        transcriber: transcriber_of(&state),
        listening: state.voice.listening(),
    })
}

async fn start_listening(State(state): State<AppState>) -> Result<StatusCode, ApiError> {
    state.voice.start()?;
    note(&state, "voice.listening", "a person", "", "");
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
struct Said {
    text: String,
}

/// Stop recording and say what was said.
async fn stop_listening(State(state): State<AppState>) -> Result<Json<Said>, ApiError> {
    let command = transcriber_of(&state);
    let text = state.voice.stop(command.as_deref())?;

    note(&state, "voice.heard", "a person", "", &text);
    Ok(Json(Said { text }))
}

#[derive(Deserialize)]
struct SetGoal {
    text: String,
}

/// What a project is for, in the words of the person who asked for it.
///
/// Kept here rather than in a pane so it survives the pane: every time a
/// commander comes back, or is traded for a fresh session, it is handed this
/// again.
async fn set_goal(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<SetGoal>,
) -> Result<Json<crate::goals::Goal>, ApiError> {
    if !state.repos.repositories().into_iter().any(|held| held.id == id) {
        return Err(anyhow::anyhow!("there is no project called {id}").into());
    }

    let goal = state
        .goals
        .set(&id, &body.text, "a person", now_secs())
        .ok_or_else(|| anyhow::anyhow!("a goal is a paragraph: not empty, and not an essay"))?;

    note(&state, "goal.set", "a person", &id, &goal.text);
    Ok(Json(goal))
}

async fn read_goals(State(state): State<AppState>) -> Json<Vec<crate::goals::Goal>> {
    Json(state.goals.everything())
}

async fn clear_goal(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if !state.goals.clear(&id) {
        return Err(anyhow::anyhow!("{id} has no goal standing").into());
    }

    note(&state, "goal.cleared", "a person", &id, "");
    Ok(StatusCode::NO_CONTENT)
}

/// What the app has been doing, and why.
async fn read_journal(
    State(state): State<AppState>,
    Query(ask): Query<crate::journal::Ask>,
) -> Json<Vec<crate::journal::Entry>> {
    Json(state.journal.read(&ask))
}

#[derive(Serialize)]
struct Allowance {
    /// `claude`, or `claude/work` when somebody has said there is more than one.
    identity: String,
    /// Which agents spend from it.
    agents: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    weekly_percent: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_percent: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    read_seconds_ago: Option<u64>,
    last_minute: crate::meter::Rate,
    ceilings: crate::meter::Ceilings,
    closest_to: &'static str,
    room: crate::budget::Room,
    says: &'static str,
}

#[derive(Serialize)]
struct BudgetReport {
    /// One per allowance. There is no single number: two subscriptions are two
    /// weeks, and one of them running out says nothing about the other.
    allowances: Vec<Allowance>,
    /// The tightest of them, for anything that wants one word.
    room: crate::budget::Room,
}

/// What the crew is allowed to spend, per allowance.
async fn read_budget(State(state): State<AppState>) -> Json<BudgetReport> {
    let now = now_secs();
    let crew = state.crew.list();

    // Every allowance anybody is hired against, plus any the engines have
    // already spoken about. An allowance with no agent on it is still worth
    // showing: it is what somebody spent yesterday.
    let mut identities: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for agent in &crew {
        identities
            .entry(identity_of(agent))
            .or_default()
            .push(agent.id.clone());
    }
    for identity in state.quota.lock().keys() {
        identities.entry(identity.clone()).or_default();
    }

    let allowances: Vec<Allowance> = identities
        .into_iter()
        .map(|(identity, agents)| {
            let held = state.quota.lock().get(&identity).copied();
            let ceilings = ceilings_for(&state, &identity);
            let last_minute = state
                .spending
                .lock()
                .get(&identity)
                .map(|window| window.in_the_last_minute(now))
                .unwrap_or_default();
            let room = room_for(&state, &identity);

            Allowance {
                weekly_percent: held.map(|(usage, _)| usage.weekly),
                session_percent: held.map(|(usage, _)| usage.session),
                read_seconds_ago: held.map(|(_, at)| now.saturating_sub(at)),
                closest_to: last_minute.tightest(&ceilings),
                identity,
                agents,
                last_minute,
                ceilings,
                room,
                says: room.in_a_line(),
            }
        })
        .collect();

    let room = allowances
        .iter()
        .map(|held| held.room)
        .fold(crate::budget::Room::Plenty, crate::meter::tighter);

    Json(BudgetReport { allowances, room })
}

#[derive(Deserialize)]
struct SetCeilings {
    /// Which allowance these belong to. Left out, they are the engine's own.
    identity: String,
    requests: u32,
    input: u64,
    /// Left out, cache reads keep the standing ceiling: it is not a number
    /// anybody has a feel for, and it is not what a plan is sold in.
    #[serde(default)]
    cached: Option<u64>,
    output: u64,
}

/// What one allowance is held to per minute. A person's to set: it is a fact
/// about their plan, not something the app can measure.
async fn set_ceilings(
    State(state): State<AppState>,
    Json(body): Json<SetCeilings>,
) -> Json<crate::meter::Ceilings> {
    let wanted = crate::meter::Ceilings {
        requests: body.requests,
        input: body.input,
        cached: body.cached.unwrap_or(crate::meter::Ceilings::default().cached),
        output: body.output,
    };

    state.ceilings.lock().insert(body.identity, wanted);
    Json(wanted)
}

#[derive(Serialize)]
struct StarterOffer {
    id: &'static str,
    label: &'static str,
    what: &'static str,
    why: &'static str,
    /// What has to be on PATH, and whether it is.
    needs: Vec<&'static str>,
    installed: bool,
    /// Which tools are missing, so the panel can name them rather than saying no.
    missing: Vec<&'static str>,
    /// What its headline package is at this moment, asked of the tool that would
    /// install it. Null when nothing could be asked — never a number this
    /// repository wrote down, because that number is wrong within the month.
    version: Option<String>,
    /// The exact commands that would run, with the name filled in. Shown before
    /// anybody presses anything: this downloads and executes other people's code.
    commands: Vec<String>,
    /// The auditor that would be run afterwards, and whether it is installed.
    audit: Option<&'static str>,
    audit_installed: bool,
    /// What can be put on top of this one, and what those are today.
    extras: Vec<ExtraOffer>,
}

#[derive(Serialize)]
struct ExtraOffer {
    id: &'static str,
    label: &'static str,
    what: &'static str,
    why: &'static str,
    version: Option<String>,
    commands: Vec<String>,
    /// What it writes into the environment, and which of those Agentland
    /// generates rather than leaves for a person to paste in.
    env: Vec<(&'static str, bool)>,
    env_file: &'static str,
}

#[derive(Deserialize)]
struct StarterQuery {
    /// The name to show in the commands. Only ever displayed — the scaffolder
    /// validates it again before anything runs.
    #[serde(default)]
    name: Option<String>,
}

/// What a new project could be made of, and what those things are today.
///
/// The versions are asked of npm and cargo at the moment the panel opens, and a
/// starter whose tools are not installed still comes back — with the tool named,
/// so a person is told what to install rather than shown a shorter list.
async fn list_starters(
    State(_state): State<AppState>,
    Query(query): Query<StarterQuery>,
) -> Json<Vec<StarterOffer>> {
    let name = query
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| crate::stacks::valid_name(value).is_ok())
        .unwrap_or("my-project")
        .to_owned();

    // Every probe is a process and a second of waiting, and there are five
    // starters; asked one after another the panel would open in five seconds.
    let asked: Vec<_> = crate::stacks::CATALOG.iter().map(|starter| {
        let name = name.clone();
        tokio::spawn(async move {
            let needs = starter.needs();
            let mut missing = Vec::new();
            for tool in &needs {
                if !crate::stacks::installed(tool).await {
                    missing.push(*tool);
                }
            }

            let auditor = starter.audit.map(|audit| audit.tool());
            let audit_installed = match starter.audit {
                Some(audit) if missing.is_empty() => crate::stacks::installed(audit.tool()).await,
                _ => false,
            };

            let version = if missing.is_empty() {
                crate::stacks::headline_version(starter).await
            } else {
                None
            };

            let commands = starter
                .steps
                .iter()
                .map(|step| {
                    format!("{} {}", step.tool, crate::stacks::fill(step.argv, &name).join(" "))
                })
                .collect();

            let mut extras = Vec::new();
            for held in crate::stacks::extras_for(starter.id) {
                let version = if missing.is_empty() {
                    crate::stacks::version_of(held.headline).await
                } else {
                    None
                };

                extras.push(ExtraOffer {
                    id: held.id,
                    label: held.label,
                    what: held.what,
                    why: held.why,
                    commands: held
                        .steps
                        .iter()
                        .map(|step| {
                            // `{version}` is resolved from what lands in
                            // node_modules, which nothing here can read yet —
                            // so the card shows the version the registry says
                            // that package is, which is what will land.
                            let argv = step.argv.join(" ").replace(
                                "{version}",
                                version.as_deref().unwrap_or("<the client's version>"),
                            );
                            format!("{} {argv}", step.tool)
                        })
                        .collect(),
                    env: held.env.to_vec(),
                    env_file: held.env_file,
                    version,
                });
            }

            StarterOffer {
                id: starter.id,
                label: starter.label,
                what: starter.what,
                why: starter.why,
                installed: missing.is_empty(),
                needs,
                missing,
                version,
                commands,
                audit: auditor,
                audit_installed,
                extras,
            }
        })
    }).collect();

    let mut offers = Vec::with_capacity(asked.len());
    for handle in asked {
        if let Ok(offer) = handle.await {
            offers.push(offer);
        }
    }

    Json(offers)
}

#[derive(Deserialize)]
struct Beginning {
    /// What the crew is being asked for. It becomes the commander's first brief.
    goal: String,
    /// A folder on this machine, or the git URL of something to clone. One of
    /// the two; a folder that is not a repository yet needs `start_git` as well.
    #[serde(default)]
    path: Option<String>,
    /// A project that does not exist yet: what to make it out of, and what to
    /// call it. `path` is then the folder to make it under.
    #[serde(default)]
    stack: Option<String>,
    #[serde(default)]
    name: Option<String>,
    /// What to put on top of the starter — authentication, and whatever else
    /// the catalog grows. Only the ones that fit the starter are accepted.
    #[serde(default)]
    extras: Vec<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    into: Option<String>,
    /// Start a git repository in the folder when there is none. Never assumed:
    /// `git init` writes into somebody's folder, so it waits for a yes.
    #[serde(default)]
    start_git: bool,
    /// Names, where the person had one in mind. Left out, each is chosen.
    #[serde(default)]
    workspace: Option<String>,
    #[serde(default)]
    worktree: Option<String>,
    #[serde(default)]
    engine_id: Option<String>,
    #[serde(default)]
    commander: Option<String>,
}

#[derive(Serialize)]
struct Begun {
    workspace: Workspace,
    repository: Repository,
    worktree: Worktree,
    commander: Agent,
    /// What this call did, in the order it did it. The panel reads it back, so
    /// a person can see which parts were made now and which were already there.
    did: Vec<String>,
    /// What the ecosystem's own auditor found in what was just installed. Only
    /// on a project this call made — nobody's existing repository gets audited
    /// because they opened it.
    #[serde(skip_serializing_if = "Option::is_none")]
    vetting: Option<crate::stacks::Vetting>,
}

/// Open a project, put a crew in it, and hand the commander the goal — one call.
///
/// Every piece of this was already possible one panel at a time: a workspace, a
/// project, a worktree, an agent, a brief. Nothing said in what order, and the
/// first thing a person met for getting it wrong was an error about a worktree
/// rather than a crew at work. Here the order is the code's problem.
///
/// Each step is skipped when what it would make is already there, so running it
/// twice is not a mistake: the second run finds the project, finds the worktree,
/// finds the commander, and hands it the new goal.
async fn begin(
    State(state): State<AppState>,
    Json(body): Json<Beginning>,
) -> Result<Json<Begun>, ApiError> {
    let goal = body.goal.trim().to_owned();
    if goal.is_empty() {
        return Err(ApiError(anyhow::anyhow!(
            "a project starts with something to do"
        )));
    }

    // Everything that can be refused for free is refused before anything is
    // made. A contradiction found afterwards still leaves the folder behind.
    if body.url.as_deref().map(str::trim).is_some_and(|value| !value.is_empty())
        && body.stack.as_deref().map(str::trim).is_some_and(|value| !value.is_empty())
    {
        return Err(ApiError(anyhow::anyhow!(
            "a project is either made here or cloned from somewhere, not both"
        )));
    }

    let mut did: Vec<String> = Vec::new();
    let known_before: Vec<String> = state
        .repos
        .repositories()
        .into_iter()
        .map(|repository| repository.id)
        .collect();

    // A project that does not exist yet is made before anything else, and what
    // it is made of decides the folder everything below then works in.
    let mut vetting = None;
    let mut made_here: Option<String> = None;

    if let Some(id) = body.stack.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        let starter = crate::stacks::starter(id)
            .ok_or_else(|| ApiError(anyhow::anyhow!("there is no starter called {id}")))?;

        let under = body
            .path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ApiError(anyhow::anyhow!("a new project needs a folder to go under")))?;

        let name = body
            .name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ApiError(anyhow::anyhow!("a new project needs a name")))?;

        // Refused before the scaffolder runs: an extra that does not fit is a
        // mistake worth catching while nothing has been written yet.
        let mut wanted = Vec::new();
        for id in &body.extras {
            let id = id.trim();
            if id.is_empty() {
                continue;
            }

            let held = crate::stacks::extra(id)
                .ok_or_else(|| ApiError(anyhow::anyhow!("there is nothing called {id} to add")))?;

            if !held.fits_starter(starter.id) {
                return Err(ApiError(anyhow::anyhow!(
                    "{} does not go on {}",
                    held.label,
                    starter.label
                )));
            }

            wanted.push(held);
        }

        let made = crate::stacks::scaffold(starter, &PathBuf::from(under), name).await?;
        did.extend(made.did);

        // Before the audit, so what an extra brought in is audited too.
        for held in &wanted {
            did.extend(crate::stacks::add(held, &made.path).await?);
        }

        // An extra that runs later writes to the same .gitignore. Nothing seen
        // so far replaces it rather than appending, and the day one does is
        // exactly the day a generated secret becomes committable — so the last
        // word on it is checked once everything has had its turn.
        for held in &wanted {
            if held.env.iter().any(|(_, generated)| *generated)
                && crate::stacks::keep_out_of_git(&made.path, held.env_file)?
            {
                did.push(format!(
                    "put {} back into .gitignore, which something else had dropped",
                    held.env_file
                ));
            }
        }

        if let Some(audit) = starter.audit {
            let found = crate::stacks::vet(audit, &made.path).await;
            did.push(found.summary.clone());
            vetting = Some(found);
        }

        made_here = Some(made.path.to_string_lossy().into_owned());
    }

    let url = body.url.as_deref().map(str::trim).filter(|value| !value.is_empty());
    let repository = match url {
        Some(url) => {
            let into = body
                .into
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| state.config.data_dir.join("clones"));
            state.repos.clone_repository(url, &into)?
        }
        None => {
            let path = made_here
                .as_deref()
                .or_else(|| {
                    body.path
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                })
                .ok_or_else(|| {
                    ApiError(anyhow::anyhow!("a project needs a folder or a git URL"))
                })?;

            // A folder Agentland just made is one it may start a repository in
            // without asking: nobody's work is in it yet, and some scaffolders
            // leave a checkout behind while others leave a plain folder.
            if body.start_git || made_here.is_some() {
                state.repos.adopt(&PathBuf::from(path))?
            } else {
                state.repos.register(&PathBuf::from(path))?
            }
        }
    };

    note(&state, "project.opened", "a person", &repository.id, &repository.name);

    did.push(if known_before.iter().any(|held| held == &repository.id) {
        format!("found {}, already open", repository.name)
    } else {
        format!("opened {}", repository.name)
    });

    let wanted_workspace = body
        .workspace
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    // A workspace nobody named is the one the person is standing in, and when
    // they are standing nowhere it is named after what they just opened.
    let workspace = match wanted_workspace {
        Some(name) => {
            let made = state.workspaces.create(CreateWorkspace {
                name: name.to_owned(),
                repository_ids: Vec::new(),
            })?;
            did.push(format!("made the {} workspace", made.name));
            made
        }
        None => {
            let (held, made) = standing_in(&state, &repository.name)?;
            if made {
                did.push(format!("made the {} workspace", held.name));
            }
            held
        }
    };

    let workspace = state.workspaces.include(&workspace.id, &repository.id)?;
    state.workspaces.activate(Some(&workspace.id))?;

    let crew = state.crew.list();
    let held = crew
        .iter()
        .find(|agent| agent.role == "commander" && agent.repository_id == repository.id)
        .cloned();

    let asked_for = body
        .worktree
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);

    let taken: Vec<String> = state
        .repos
        .worktrees()
        .into_iter()
        .filter(|entry| entry.worktree.repository_id == repository.id)
        .map(|entry| entry.worktree.name)
        .collect();

    let name = asked_for.unwrap_or_else(|| crate::start::worktree_name(&goal, &taken));

    let standing = state.repos.worktrees().into_iter().find(|entry| {
        entry.worktree.repository_id == repository.id && entry.worktree.name == name
    });

    let worktree = match standing {
        Some(entry) => entry.worktree,
        None => {
            let made = state.repos.create_worktree(&repository.id, &name)?;
            did.push(format!("cut the {} worktree on {}", made.name, made.branch));
            made
        }
    };

    // The commander sits at its own desk rather than in the branch the work
    // commits to: a branch is checked out in one place, and a commander parked
    // there is a commander standing where the implementer has to be.
    let desk = desk_for(&state, &repository.id)?;

    let commander = match held {
        Some(commander) => commander,
        None => {
            let engine_id = match body
                .engine_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                Some(chosen) => chosen.to_owned(),
                None => crate::start::engine_for_a_commander(&crate::crew::engines()).ok_or_else(
                    || {
                        ApiError(anyhow::anyhow!(
                            "no coding agent is installed — put one on PATH and start again"
                        ))
                    },
                )?,
            };

            let ids: Vec<String> = crew.iter().map(|agent| agent.id.clone()).collect();
            let hired = take_on(
                &state,
                HireRequest {
                    name: crate::start::commander_name(body.commander.as_deref(), &ids),
                    role: "commander".to_owned(),
                    engine_id,
                    repository_id: repository.id.clone(),
                    worktree: desk.name.clone(),
                    model: None,
                    title: None,
                    colour: None,
                    permissions: None,
                    account: None,
                },
            )?;

            did.push(format!("hired {} to command, on {}", hired.name, hired.engine_id));
            hired
        }
    };

    let sitting = state
        .repos
        .worktrees()
        .into_iter()
        .find(|entry| {
            entry.worktree.repository_id == commander.repository_id
                && entry.worktree.name == commander.worktree
        })
        .map(|entry| entry.worktree)
        .unwrap_or_else(|| desk.clone());

    let brief = compose_brief(&state, &commander, &goal).await;
    match hand_the_work_over(&state, &commander, &sitting.path, &brief).await? {
        HandOver::Busy => {
            return Err(ApiError(anyhow::anyhow!(
                "{} is in the middle of a turn — wait for it, then start again",
                commander.name
            )))
        }
        HandOver::Started => did.push(format!("started {} on the goal", commander.name)),
        HandOver::Typed => did.push(format!("gave {} the goal", commander.name)),
    }

    // Read back rather than reused: the pane it was just given is what the
    // panel opens, and the copy from before it started does not carry one.
    let commander = state
        .crew
        .list()
        .into_iter()
        .find(|agent| agent.id == commander.id)
        .unwrap_or(commander);

    state.notices.push(
        crate::notices::NewNotice {
            kind: crate::notices::Kind::Word,
            text: format!("{} is on: {goal}", commander.name),
            repository_id: Some(repository.id.clone()),
            agent_id: Some(commander.id.clone()),
            opens: Some("commander".to_owned()),
            ..Default::default()
        },
        now_secs(),
    );

    Ok(Json(Begun {
        workspace,
        repository,
        worktree,
        commander,
        did,
        vetting,
    }))
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

    let (plan, just_finished) = state
        .plans
        .mark_and_notice(&id, &step, body.state, body.note)?;

    if just_finished {
        state.leader_words.lock().push(plan_finished_word(&plan));
        state.notices.push(
            crate::notices::NewNotice {
                kind: crate::notices::Kind::Finished,
                text: format!("Plan finished: {}", plan.goal.trim()),
                repository_id: Some(plan.repository_id.clone()),
                agent_id: Some(plan.created_by.clone()),
                opens: Some("commander".to_owned()),
                ..Default::default()
            },
            now_secs(),
        );
    }

    Ok(Json(plan))
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

#[cfg(test)]
mod presence_tests {
    use super::presence_name;

    #[test]
    fn an_agent_mid_turn_is_working() {
        assert_eq!(presence_name(false, true, true, false).0, "working");
    }

    #[test]
    fn an_agent_at_an_empty_prompt_is_waiting_rather_than_working() {
        // The pane keeps printing its status line, so the clock would say
        // "working" forever; the pane itself says otherwise.
        assert_eq!(presence_name(false, true, false, false).0, "waiting");
    }

    #[test]
    fn an_approval_outranks_whatever_the_pane_shows() {
        assert_eq!(presence_name(true, true, true, false).0, "attention");
        assert_eq!(presence_name(true, false, false, true).0, "attention");
    }

    #[test]
    fn a_closed_session_is_done_if_it_ran_and_idle_if_it_never_did() {
        assert_eq!(presence_name(false, false, false, true).0, "done");
        assert_eq!(presence_name(false, false, false, false).0, "idle");
    }
}


#[cfg(test)]
mod plan_word_tests {
    use super::plan_finished_word;
    use crate::plans::{Plan, PlanState, Step, StepState};

    fn plan() -> Plan {
        Plan {
            id: "p1".into(),
            goal: "svc-demo answers /health".into(),
            repository_id: "demo".into(),
            created_by: "x".into(),
            state: PlanState::Done,
            steps: vec![
                Step {
                    id: "p1s1".into(),
                    title: "Serve /health".into(),
                    brief: String::new(),
                    needs: vec![],
                    task_id: None,
                    note: Some("the port comes from PORT".into()),
                    state: StepState::Done,
                },
                Step {
                    id: "p1s2".into(),
                    title: "Prove it".into(),
                    brief: String::new(),
                    needs: vec!["p1s1".into()],
                    task_id: None,
                    note: None,
                    state: StepState::Done,
                },
            ],
        }
    }

    #[test]
    fn it_asks_for_the_note_and_hands_over_the_evidence() {
        let word = plan_finished_word(&plan());

        assert!(word.contains("note_write"), "it asks for a note: {word}");
        assert!(word.contains("svc-demo answers /health"), "it names the plan");
        assert!(word.contains("Serve /health"), "it lists the steps");
        assert!(word.contains("the port comes from PORT"), "a step's note is evidence");
        assert!(word.contains("2 steps done"), "it says how much was done");
    }

    #[test]
    fn one_step_is_not_called_steps() {
        let mut single = plan();
        single.steps.truncate(1);
        assert!(plan_finished_word(&single).contains("1 step done"));
    }
}

#[cfg(test)]
mod hand_over_tests {
    use crate::supervisor::{asking_the_human, turn_running};

    /// The two readings `hand_the_work_over` leans on, checked against the panes
    /// they were written from. A pane that is neither running nor asking is a
    /// pane that can be handed the next step where it stands.
    #[test]
    fn an_idle_pane_is_ready_for_the_next_step() {
        let resting = "✻ Baked for 22s · done 10:04 PM\n❯\n⏵⏵ bypass permissions on (shift+tab to cycle)";

        assert!(!turn_running(resting));
        assert!(!asking_the_human(resting));
    }

    #[test]
    fn a_pane_mid_turn_is_left_alone() {
        let working = "● Bash(npm test)\n  ⎿  Running…\n✢ Sprouting… (28s · ↓ 2.1k tokens)";

        assert!(turn_running(working));
    }

    #[test]
    fn a_pane_holding_a_question_is_left_alone_too() {
        let asking = "This command requires approval\nDo you want to proceed?\n❯ 1. Yes\nEsc to cancel · Tab to amend";

        assert!(asking_the_human(asking));
    }
}

#[cfg(test)]
mod news_tests {
    use crate::supervisor::{Watch, WatchState};

    fn settled(step_id: &str, task_id: &str) -> Watch {
        Watch {
            id: "w1".to_owned(),
            plan_id: String::new(),
            step_id: step_id.to_owned(),
            task_id: task_id.to_owned(),
            agent_id: "ada".to_owned(),
            session_id: "pane-1".to_owned(),
            repository_id: "svc".to_owned(),
            worktree: "ada-tree".to_owned(),
            fingerprint: "close idle connections".to_owned(),
            delivered: true,
            resends: 0,
            state: WatchState::Settled,
            started_at: 0,
            settled_at: 100,
            reason: Some("ada attached evidence".to_owned()),
            told_leader: false,
            wake_attempts: 0,
            last_wake: 0,
            reaped: false,
            worked: true,
        }
    }

    #[test]
    fn a_card_with_no_plan_behind_it_names_itself() {
        let text = super::news_text(&[settled("", "t358")]);

        assert!(text.contains("t358"), "the card names itself: {text}");
        assert!(!text.contains("()"), "no empty brackets where a step would be: {text}");
    }

    #[test]
    fn a_plan_step_still_names_both() {
        let text = super::news_text(&[settled("p12s2", "t359")]);

        assert!(text.contains("p12s2 (t359)"), "{text}");
    }
}

#[cfg(test)]
mod leaving_tests {
    use super::{holding_says, Holdings};

    fn holding(cards: usize, pane: bool, uncommitted: usize, unpushed: u32) -> Holdings {
        Holdings {
            cards: (0..cards)
                .map(|n| super::HeldCard {
                    id: format!("t{n}"),
                    title: "something".to_owned(),
                    column: crate::board::Column::Working,
                })
                .collect(),
            pane_running: pane,
            uncommitted,
            unpushed,
            worktree: None,
            empty_handed: cards == 0 && !pane && uncommitted == 0 && unpushed == 0,
        }
    }

    #[test]
    fn empty_hands_say_nothing_so_the_question_is_not_dressed_up_as_a_warning() {
        assert_eq!(holding_says(&holding(0, false, 0, 0)), None);
    }

    #[test]
    fn work_that_exists_nowhere_else_is_named_rather_than_summed() {
        let says = holding_says(&holding(2, false, 3, 1)).expect("it is holding something");

        assert!(says.contains("2 unfinished card(s)"));
        assert!(says.contains("3 uncommitted file(s)"));
        assert!(says.contains("1 commit(s) not on the base branch"));
    }

    #[test]
    fn an_open_pane_counts_because_letting_somebody_go_closes_it() {
        assert_eq!(
            holding_says(&holding(0, true, 0, 0)).as_deref(),
            Some("a pane still open")
        );
    }
}
