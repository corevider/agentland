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
    cwd: string | null;
    started_at: number;
    last_output_at: number;
    bytes: number;
    lines: number;
    context_percent: number | null;
    context_tokens: number | null;
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
    island_fps?: number;
    island_worst_ms?: number;
    panels?: string;
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

export type AgentState = "idle" | "working" | "done" | "offline";
export type Presence = "idle" | "working" | "done" | "attention";

export interface Agent {
    id: string;
    name: string;
    role: string;
    engine_id: string;
    repository_id: string;
    worktree: string;
    session_id: string | null;
    state: AgentState;
    presence: Presence;
    since: number;
    reason: string;
}

export interface HireRequest {
    name: string;
    role: string;
    engine_id: string;
    repository_id: string;
    worktree: string;
}

export interface Skill {
    id: string;
    name: string;
    description: string;
    when_to_use: string;
    body: string;
    builtin: boolean;
}

export interface Workspace {
    id: string;
    name: string;
    repository_ids: string[];
}

export interface WorkspaceList {
    workspaces: Workspace[];
    active: string | null;
}

export function list_workspaces(): Promise<WorkspaceList> {
    return request<WorkspaceList>("/workspaces");
}

export function create_workspace(name: string, repository_ids: string[]): Promise<Workspace> {
    return request<Workspace>("/workspaces", {
        method: "POST",
        body: JSON.stringify({ name, repository_ids }),
    });
}

export function activate_workspace(id: string | null): Promise<Workspace | null> {
    return request<Workspace | null>("/workspaces/active", {
        method: "POST",
        body: JSON.stringify({ id }),
    });
}

export function set_workspace_repos(id: string, repository_ids: string[]): Promise<Workspace> {
    return request<Workspace>(`/workspaces/${encodeURIComponent(id)}`, {
        method: "POST",
        body: JSON.stringify({ repository_ids }),
    });
}

export function remove_workspace(id: string): Promise<void> {
    return request<void>(`/workspaces/${encodeURIComponent(id)}`, { method: "DELETE" });
}

export interface MailMessage {
    id: string;
    from: string;
    to: string;
    text: string;
    delivered: boolean;
}

export interface MailPolicy {
    paused: boolean;
    allow_unlisted: boolean;
    grants: Record<string, string[]>;
}

export function list_mail(): Promise<MailMessage[]> {
    return request<MailMessage[]>("/mail");
}

export function send_mail(from: string, to: string, text: string): Promise<MailMessage> {
    return request<MailMessage>("/mail", {
        method: "POST",
        body: JSON.stringify({ from, to, text }),
    });
}

export function mail_policy(): Promise<MailPolicy> {
    return request<MailPolicy>("/mail/policy");
}

export function set_mail_policy(policy: MailPolicy): Promise<MailPolicy> {
    return request<MailPolicy>("/mail/policy", {
        method: "POST",
        body: JSON.stringify(policy),
    });
}

export type MemoryScope = "workspace" | "repository" | "agent";

export interface Memory {
    id: string;
    text: string;
    scope: MemoryScope;
    scope_id: string;
    proposed_by: string;
    approved: boolean;
    masked: boolean;
}

export function list_memories(): Promise<Memory[]> {
    return request<Memory[]>("/memories");
}

export function propose_memory(
    text: string,
    scope: MemoryScope,
    scope_id: string,
    proposed_by: string,
): Promise<Memory> {
    return request<Memory>("/memories", {
        method: "POST",
        body: JSON.stringify({ text, scope, scope_id, proposed_by }),
    });
}

export function answer_memory(id: string, approved: boolean): Promise<Memory> {
    return request<Memory>(`/memories/${encodeURIComponent(id)}/approve`, {
        method: "POST",
        body: JSON.stringify({ approved }),
    });
}

export function forget_memory(id: string): Promise<void> {
    return request<void>(`/memories/${encodeURIComponent(id)}`, { method: "DELETE" });
}

export interface Routine {
    id: string;
    name: string;
    agent_id: string;
    brief: string;
    every_minutes: number;
    draft_only: boolean;
    enabled: boolean;
    last_run: number;
    consecutive_failures: number;
    last_result: string | null;
}

export function list_routines(): Promise<Routine[]> {
    return request<Routine[]>("/routines");
}

export function create_routine(payload: {
    name: string;
    agent_id: string;
    brief: string;
    every_minutes: number;
    draft_only: boolean;
}): Promise<Routine> {
    return request<Routine>("/routines", { method: "POST", body: JSON.stringify(payload) });
}

export function set_routine_enabled(id: string, enabled: boolean): Promise<Routine> {
    return request<Routine>(`/routines/${encodeURIComponent(id)}/enabled`, {
        method: "POST",
        body: JSON.stringify({ enabled }),
    });
}

export function delete_routine(id: string): Promise<void> {
    return request<void>(`/routines/${encodeURIComponent(id)}`, { method: "DELETE" });
}

export interface DispatchEvent {
    seq: number;
    agent_id: string;
    task_id: string;
    reason: string;
}

export interface DispatchCaps {
    per_repository: number;
    per_engine: number;
}

export interface DispatchState {
    paused: boolean;
    caps: DispatchCaps;
    queue: string[];
    events: DispatchEvent[];
    next_seq: number;
}

export type DispatchDecision =
    | { outcome: "assign"; agent_id: string; reason: string }
    | { outcome: "queue"; reason: string }
    | { outcome: "refuse"; reason: string };

export interface DispatchReport {
    state: DispatchState;
    decision: DispatchDecision;
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

export function set_dispatch_caps(caps: DispatchCaps): Promise<DispatchState> {
    return request<DispatchState>("/dispatch/caps", {
        method: "POST",
        body: JSON.stringify(caps),
    });
}

export function dispatch_task(id: string): Promise<DispatchReport> {
    return request<DispatchReport>(`/dispatch/tasks/${encodeURIComponent(id)}`, { method: "POST" });
}

export function list_skills(): Promise<Skill[]> {
    return request<Skill[]>("/skills");
}

export function write_skill(id: string, manifest: string): Promise<Skill> {
    return request<Skill>("/skills", {
        method: "POST",
        body: JSON.stringify({ id, manifest }),
    });
}

export function remove_skill(id: string): Promise<void> {
    return request<void>(`/skills/${encodeURIComponent(id)}`, { method: "DELETE" });
}

export function agent_skills(agent_id: string): Promise<Skill[]> {
    return request<Skill[]>(`/agents/${encodeURIComponent(agent_id)}/skills`);
}

export function install_skill(agent_id: string, skill_id: string): Promise<Skill[]> {
    return request<Skill[]>(`/agents/${encodeURIComponent(agent_id)}/skills`, {
        method: "POST",
        body: JSON.stringify({ skill_id }),
    });
}

export function uninstall_skill(agent_id: string, skill_id: string): Promise<Skill[]> {
    return request<Skill[]>(
        `/agents/${encodeURIComponent(agent_id)}/skills/${encodeURIComponent(skill_id)}`,
        { method: "DELETE" },
    );
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

export function take_ui_commands(): Promise<string[]> {
    return request<string[]>("/ui/commands");
}

export function read_log(session_id: string, bytes = 2400): Promise<string> {
    return request<string>(`/sessions/${session_id}/log?bytes=${bytes}`);
}

export function answer_approval(id: string, approved: boolean, note?: string): Promise<Approval> {
    return request<Approval>(`/approvals/${id}`, {
        method: "POST",
        body: JSON.stringify({ approved, note }),
    });
}

export interface Approval {
    id: string;
    summary: string;
    detail: string;
    requested_by: string;
    verdict: "pending" | "approved" | "rejected";
    answered_note: string | null;
}

export function list_approvals(): Promise<Approval[]> {
    return request<Approval[]>("/approvals");
}
