export interface CoreEndpoint {
    host: string;
    port: number;
    token: string;
}

export interface SessionInfo {
    id: string;
    kind: string;
    command: string;
    cols: number;
    rows: number;
    started_at: number;
    last_output_at: number;
    bytes: number;
    lines: number;
    context_percent: number | null;
    alive: boolean;
}

export function format_elapsed(seconds: number): string {
    if (seconds < 60) {
        return `${Math.max(seconds, 0)}s`;
    }
    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) {
        return `${minutes}m ${seconds % 60}s`;
    }
    return `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
}

export function format_bytes(bytes: number): string {
    if (bytes < 1024) {
        return `${bytes} B`;
    }
    if (bytes < 1024 * 1024) {
        return `${(bytes / 1024).toFixed(1)} KB`;
    }
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function session_stats(id: string): Promise<SessionInfo> {
    return request<SessionInfo>(`/sessions/${id}/stats`);
}

export interface GeneratorSpec {
    lines_per_second: number;
    duration_ms: number;
    line_width: number;
    colored: boolean;
}

let endpoint: CoreEndpoint | null = null;

export function is_tauri(): boolean {
    const scope = window as unknown as Record<string, unknown>;
    return "__TAURI_INTERNALS__" in scope || "__TAURI__" in scope;
}

export async function resolve_endpoint(): Promise<CoreEndpoint> {
    if (endpoint) {
        return endpoint;
    }

    if (is_tauri()) {
        const { invoke } = await import("@tauri-apps/api/core");
        endpoint = await invoke<CoreEndpoint>("core_endpoint");
        return endpoint;
    }

    const params = new URLSearchParams(window.location.search);
    endpoint = {
        host: "127.0.0.1",
        port: Number(params.get("port") ?? 9470),
        token: params.get("token") ?? "",
    };
    return endpoint;
}

function base_url(target: CoreEndpoint): string {
    return `http://${target.host}:${target.port}`;
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
    const target = await resolve_endpoint();
    const response = await fetch(`${base_url(target)}${path}`, {
        ...init,
        headers: {
            "content-type": "application/json",
            "x-auth-token": target.token,
            ...(init?.headers ?? {}),
        },
    });

    if (!response.ok) {
        const detail = await response.text();
        throw new Error(`${response.status} ${detail}`);
    }

    if (response.status === 204) {
        return undefined as T;
    }

    return (await response.json()) as T;
}

export function list_sessions(): Promise<SessionInfo[]> {
    return request<SessionInfo[]>("/sessions");
}

export function spawn_shell(command: string, cwd?: string): Promise<SessionInfo> {
    return request<SessionInfo>("/sessions", {
        method: "POST",
        body: JSON.stringify({ command, args: [], cwd, cols: 120, rows: 32 }),
    });
}

export function spawn_generator(spec: GeneratorSpec): Promise<SessionInfo> {
    return request<SessionInfo>("/bench", {
        method: "POST",
        body: JSON.stringify(spec),
    });
}

export function kill_session(id: string): Promise<void> {
    return request<void>(`/sessions/${id}`, { method: "DELETE" });
}

export function write_input(id: string, data: string): Promise<void> {
    return request<void>(`/sessions/${id}/input`, {
        method: "POST",
        body: JSON.stringify({ data }),
    });
}

export function resize_session(id: string, cols: number, rows: number): Promise<void> {
    return request<void>(`/sessions/${id}/resize`, {
        method: "POST",
        body: JSON.stringify({ cols, rows }),
    });
}

export interface Sample {
    run_id: string;
    elapsed_ms: number;
    panes: number;
    lines_per_second: number;
    fps: number;
    worst_frame_ms: number;
    mb_per_second: number;
    dropped_frames: number;
    dropped_local: number;
    renderer: string;
    surface: string;
    gpu: string;
}

export function report_sample(sample: Sample): Promise<void> {
    return request<void>("/metrics", {
        method: "POST",
        body: JSON.stringify(sample),
    });
}

export async function open_stream(id: string): Promise<WebSocket> {
    const target = await resolve_endpoint();
    const url = `ws://${target.host}:${target.port}/sessions/${id}/stream?token=${encodeURIComponent(target.token)}`;
    const socket = new WebSocket(url);
    socket.binaryType = "arraybuffer";
    return socket;
}

export interface Remote {
    name: string;
    url: string;
    host: string | null;
    owner: string | null;
    repo: string | null;
    provider: string;
}

export interface Repository {
    id: string;
    name: string;
    primary_path: string;
    default_branch: string;
    remotes: Remote[];
    origin: string | null;
}

export interface WorktreeStatus {
    name: string;
    repository_id: string;
    path: string;
    branch: string;
    port: number;
    dirty_files: number;
    ahead: number;
    missing: boolean;
}

export function list_repos(): Promise<Repository[]> {
    return request<Repository[]>("/repos");
}

export function add_repo(path: string): Promise<Repository> {
    return request<Repository>("/repos", {
        method: "POST",
        body: JSON.stringify({ path }),
    });
}

export function list_worktrees(repository_id: string): Promise<WorktreeStatus[]> {
    return request<WorktreeStatus[]>(`/repos/${repository_id}/worktrees`);
}

export function create_worktree(repository_id: string, name: string): Promise<WorktreeStatus> {
    return request<WorktreeStatus>(`/repos/${repository_id}/worktrees`, {
        method: "POST",
        body: JSON.stringify({ name }),
    });
}

export function remove_worktree(repository_id: string, name: string, force: boolean): Promise<void> {
    const suffix = force ? "?force=true" : "";
    return request<void>(`/repos/${repository_id}/worktrees/${name}${suffix}`, { method: "DELETE" });
}

export type ServiceState = "starting" | "ready" | "unreachable" | "stopped";

export interface Service {
    key: string;
    repository_id: string;
    worktree: string;
    port: number;
    session_id: string;
    state: ServiceState;
    command: string;
    detected_from: string;
    url: string;
}

export function list_services(): Promise<Service[]> {
    return request<Service[]>("/services");
}

export function start_service(repository_id: string, worktree: string): Promise<Service> {
    return request<Service>(`/repos/${repository_id}/worktrees/${worktree}/service`, {
        method: "POST",
    });
}

export function stop_service(repository_id: string, worktree: string): Promise<void> {
    return request<void>(`/repos/${repository_id}/worktrees/${worktree}/service`, {
        method: "DELETE",
    });
}

export interface Engine {
    id: string;
    name: string;
    command: string;
    resume_flag: string | null;
    installed: boolean;
    version: string | null;
}

export type AgentState = "idle" | "working" | "offline";

export interface Agent {
    id: string;
    name: string;
    role: string;
    engine_id: string;
    repository_id: string;
    worktree: string;
    session_id: string | null;
    state: AgentState;
}

export interface HireRequest {
    name: string;
    role: string;
    engine_id: string;
    repository_id: string;
    worktree: string;
}

export function list_engines(): Promise<Engine[]> {
    return request<Engine[]>("/engines");
}

export function list_agents(): Promise<Agent[]> {
    return request<Agent[]>("/agents");
}

export function hire_agent(payload: HireRequest): Promise<Agent> {
    return request<Agent>("/agents", { method: "POST", body: JSON.stringify(payload) });
}

export function start_agent(id: string, resume: boolean): Promise<Agent> {
    return request<Agent>(`/agents/${id}/start${resume ? "?resume=true" : ""}`, { method: "POST" });
}

export function stop_agent(id: string): Promise<void> {
    return request<void>(`/agents/${id}/stop`, { method: "POST" });
}

export function dismiss_agent(id: string): Promise<void> {
    return request<void>(`/agents/${id}`, { method: "DELETE" });
}

export type Column = "backlog" | "assigned" | "working" | "review" | "done";

export interface Evidence {
    kind: string;
    [key: string]: unknown;
}

export interface Task {
    id: string;
    title: string;
    body: string;
    column: Column;
    repository_id: string;
    assignee: string | null;
    worktree: string | null;
    branch: string | null;
    evidence: Evidence[];
}

export interface CommitInfo {
    sha: string;
    subject: string;
}

export interface Review {
    base: string;
    branch: string;
    files: number;
    insertions: number;
    deletions: number;
    commits: CommitInfo[];
    untracked: string[];
    uncommitted: boolean;
    patch: string;
}

export interface PullRequestResult {
    url: string;
    created: boolean;
    detail: string;
}

export function list_tasks(): Promise<Task[]> {
    return request<Task[]>("/tasks");
}

export function create_task(title: string, body: string, repository_id: string): Promise<Task> {
    return request<Task>("/tasks", {
        method: "POST",
        body: JSON.stringify({ title, body, repository_id }),
    });
}

export function move_task(id: string, column: Column): Promise<Task> {
    return request<Task>(`/tasks/${id}/move`, { method: "POST", body: JSON.stringify({ column }) });
}

export function assign_task(id: string, agent_id: string): Promise<Task> {
    return request<Task>(`/tasks/${id}/assign`, {
        method: "POST",
        body: JSON.stringify({ agent_id }),
    });
}

export function delete_task(id: string): Promise<void> {
    return request<void>(`/tasks/${id}`, { method: "DELETE" });
}

export function review_worktree(repository_id: string, worktree: string): Promise<Review> {
    return request<Review>(`/repos/${repository_id}/worktrees/${worktree}/review`);
}

export function open_pull_request(
    repository_id: string,
    worktree: string,
    title: string,
    body: string,
    task_id?: string,
): Promise<PullRequestResult> {
    return request<PullRequestResult>(`/repos/${repository_id}/worktrees/${worktree}/pr`, {
        method: "POST",
        body: JSON.stringify({ title, body, task_id }),
    });
}

export interface DispatchState {
    paused: boolean;
    caps: { per_repository: number; per_engine: number };
    queue: string[];
}

export interface DispatchReport {
    state: DispatchState;
    decision: { outcome: "assign" | "queue" | "refuse"; agent_id?: string; reason: string };
    task: Task | null;
}

export function dispatch_status(): Promise<DispatchState> {
    return request<DispatchState>("/dispatch");
}

export function pause_dispatch(paused: boolean): Promise<DispatchState> {
    return request<DispatchState>("/dispatch/pause", {
        method: "POST",
        body: JSON.stringify({ paused }),
    });
}

export function dispatch_task(id: string): Promise<DispatchReport> {
    return request<DispatchReport>(`/dispatch/tasks/${id}`, { method: "POST" });
}
