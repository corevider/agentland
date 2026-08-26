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

export interface Repository {
    id: string;
    name: string;
    primary_path: string;
    default_branch: string;
    remote: string | null;
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
