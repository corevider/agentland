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

use crate::bench::GeneratorSpec;
use crate::metrics::{MetricsStore, Sample};
use crate::repo::{RepoRegistry, Repository, Worktree, WorktreeStatus};
use crate::pty::{PtyManager, PtySpawnSpec, SessionInfo};

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
    };

    let app = Router::new()
        .route("/sessions", get(list_sessions).post(spawn_session))
        .route("/sessions/{id}", delete(kill_session))
        .route("/sessions/{id}/input", post(write_input))
        .route("/sessions/{id}/resize", post(resize_session))
        .route("/sessions/{id}/stream", get(stream_session))
        .route("/sessions/{id}/log", get(read_log))
        .route("/bench", post(spawn_generator))
        .route("/metrics", get(read_metrics).post(record_metrics))
        .route("/repos", get(list_repos).post(add_repo))
        .route("/repos/{id}/worktrees", get(list_worktrees).post(create_worktree))
        .route("/repos/{id}/worktrees/{name}", delete(remove_worktree))
        .route("/ports", get(list_ports))
        .layer(middleware::from_fn_with_state(state.clone(), guard))
        .layer(cors)
        .with_state(state);

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

    let header_token = headers
        .get("x-auth-token")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let token = header_token.or(query.token).unwrap_or_default();

    if token != state.config.token {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ErrorBody {
                error: "invalid token".into(),
            }),
        )
            .into_response();
    }

    next.run(request).await
}

async fn list_sessions(State(state): State<AppState>) -> Json<Vec<SessionInfo>> {
    Json(state.manager.list())
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
