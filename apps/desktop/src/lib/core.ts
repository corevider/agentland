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

/// Take a folder as a project. `start_git` says yes to starting a repository in
/// a folder that is not one yet — it writes to the folder, so the panel asks
/// before passing it.
export function add_repo(path: string, start_git = false): Promise<Repository> {
    return request<Repository>("/repos", {
        method: "POST",
        body: JSON.stringify({ path, start_git }),
    });
}

/// Stop tracking a project. The folder on disk is left alone.
export async function forget_repo(id: string): Promise<void> {
    await request<void>(`/repos/${id}`, { method: "DELETE" });
}

/// Clone a repository into a folder the person picked.
export function clone_repo(url: string, into: string): Promise<Repository> {
    return request<Repository>("/repos", {
        method: "POST",
        body: JSON.stringify({ url, into }),
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

export interface Worktree {
    name: string;
    repository_id: string;
    path: string;
    branch: string;
    port: number;
}

/// What a new project could be made of, and what that is today.
export interface Starter {
    id: string;
    label: string;
    what: string;
    why: string;
    needs: string[];
    installed: boolean;
    missing: string[];
    /// The headline package's version, asked of the tool that would install it.
    /// Null when nothing could be asked — never a number the app wrote down.
    version: string | null;
    /// Exactly what would run. Shown before anything does: this downloads and
    /// executes other people's code.
    commands: string[];
    audit: string | null;
    audit_installed: boolean;
    /// What can be put on top of this one.
    extras: StarterExtra[];
}

/// Something added to a starter — authentication, and whatever the catalog
/// grows next.
export interface StarterExtra {
    id: string;
    label: string;
    what: string;
    why: string;
    version: string | null;
    commands: string[];
    /// Each name, and whether the core generates it rather than leaving it for
    /// a person to paste in.
    env: [string, boolean][];
    env_file: string;
}

export function list_starters(name?: string): Promise<Starter[]> {
    const suffix = name ? `?name=${encodeURIComponent(name)}` : "";
    return request<Starter[]>(`/stacks${suffix}`);
}

/// What the ecosystem's own auditor found. `ran: false` means nobody looked,
/// which is not the same as nothing being wrong.
export interface Vetting {
    tool: string;
    summary: string;
    ran: boolean;
}

/// What a project needs to begin: somewhere to work, and something to do.
export interface Beginning {
    goal: string;
    path?: string;
    url?: string;
    into?: string;
    /// Say yes to `git init` in a folder that is not a repository yet.
    start_git?: boolean;
    /// A project that does not exist yet: what to make it out of, and what to
    /// call it. `path` is then the folder it goes under.
    stack?: string;
    name?: string;
    extras?: string[];
    workspace?: string;
    worktree?: string;
    engine_id?: string;
    commander?: string;
}

export interface Begun {
    workspace: Workspace;
    repository: Repository;
    worktree: Worktree;
    commander: Agent;
    /// What the core did, in order, so the panel can say it back rather than
    /// claiming more than happened.
    did: string[];
    /// Only on a project this call made — an existing repository is not audited
    /// because somebody opened it.
    vetting?: Vetting;
}

/// Open a project, put a crew in it and give the commander the goal, in one call.
///
/// Running it again on a project that is already open is not a mistake: whatever
/// is already there is found rather than made, and the commander is handed the
/// new goal.
export type Room = "plenty" | "tight" | "spent";

export interface Rate {
    requests: number;
    input: number;
    /// Read back from the cache, kept apart: it is most of the traffic here and
    /// counting it as input made a pane at 10% of its week read as spent.
    cached: number;
    output: number;
}

export interface Ceilings {
    requests: number;
    input: number;
    cached: number;
    output: number;
}

/// One subscription's worth of allowance.
///
/// `identity` is the engine, and a login within it when somebody has said there
/// is more than one — `claude`, `claude/work`, `codex`. The weekly numbers are
/// read off that engine's own status line, so they are absent until one of its
/// panes has been open long enough to say. The minute is counted from what its
/// engines wrote in their transcripts.
export interface Allowance {
    identity: string;
    agents: string[];
    weekly_percent?: number;
    session_percent?: number;
    read_seconds_ago?: number;
    last_minute: Rate;
    ceilings: Ceilings;
    closest_to: string;
    room: Room;
    says: string;
}

/// There is no single number here on purpose: two subscriptions are two weeks,
/// and one running out says nothing about the other.
export interface Budget {
    allowances: Allowance[];
    room: Room;
}

export function read_budget(): Promise<Budget> {
    return request<Budget>("/budget");
}

/// The cache-read ceiling is not offered: it is not a number anybody has a feel
/// for, and left out the core keeps its own. Sending a zero would read as a
/// ceiling of nothing, which is every allowance spent forever.
export function set_ceilings(
    identity: string,
    ceilings: Omit<Ceilings, "cached">,
): Promise<Ceilings> {
    return request<Ceilings>("/budget", {
        method: "POST",
        body: JSON.stringify({ identity, ...ceilings }),
    });
}

/// What a project's agents may do without stopping to ask.
export interface ProjectPermits {
    repository_id: string;
    rules: string[];
    running: string[];
}

export function read_permits(): Promise<ProjectPermits[]> {
    return request<ProjectPermits[]>("/permits");
}

export function forget_permit(repository_id: string, rule: string): Promise<void> {
    return request<void>("/permits", {
        method: "DELETE",
        body: JSON.stringify({ repository_id, rule }),
    });
}

/// One thing the app decided or did.
export interface JournalEntry {
    at: number;
    kind: string;
    actor: string;
    subject: string;
    detail: string;
}

export function read_journal(ask: { kind?: string; limit?: number } = {}): Promise<JournalEntry[]> {
    const query = new URLSearchParams();
    if (ask.kind) {
        query.set("kind", ask.kind);
    }
    query.set("limit", String(ask.limit ?? 120));
    return request<JournalEntry[]>(`/journal?${query}`);
}

export interface Ignited {
    commander: Agent;
    worktree: Worktree;
    did: string[];
}

/// Put a project's commander at its desk and set it going.
///
/// The same call whether there is nobody yet, somebody stopped, or somebody
/// already working — the core decides which of the three it is.
/// How to get a phone in: an address that carries the token, and that address
/// drawn as something a camera can read.
export interface PhoneWayIn {
    urls: string[];
    code?: string;
    reachable: boolean;
}

export function phone_way_in(): Promise<PhoneWayIn> {
    return request<PhoneWayIn>("/phone");
}

/// How the house works: handed to every agent, in every project, for every
/// turn — not repeated in each brief.
export interface HouseRules {
    text: string;
    held: boolean;
}

export function read_standards(): Promise<HouseRules> {
    return request<HouseRules>("/standards");
}

export function set_standards(text: string): Promise<HouseRules> {
    return request<HouseRules>("/standards", {
        method: "POST",
        body: JSON.stringify({ text }),
    });
}

/// Speaking to the crew instead of typing to it. The recorder is whatever is
/// already on the machine; the transcriber is a command somebody set.
export interface VoiceState {
    recorder?: string;
    transcriber?: string;
    listening: boolean;
}

export function voice_state(): Promise<VoiceState> {
    return request<VoiceState>("/voice");
}

export function set_transcriber(command: string): Promise<VoiceState> {
    return request<VoiceState>("/voice", {
        method: "POST",
        body: JSON.stringify({ command }),
    });
}

export function start_listening(): Promise<void> {
    return request<void>("/voice/start", { method: "POST" });
}

export function stop_listening(): Promise<{ text: string }> {
    return request<{ text: string }>("/voice/stop", { method: "POST" });
}

/// What a project is for, in the words of the person who asked.
///
/// Kept by the core rather than in a pane, because a pane traded for a fresh
/// one takes everything said to it with it.
export interface Goal {
    repository_id: string;
    text: string;
    set_by: string;
    at: number;
}

export function read_goals(): Promise<Goal[]> {
    return request<Goal[]>("/goals");
}

export function set_goal(repository_id: string, text: string): Promise<Goal> {
    return request<Goal>(`/repos/${encodeURIComponent(repository_id)}/goal`, {
        method: "POST",
        body: JSON.stringify({ text }),
    });
}

export function clear_goal(repository_id: string): Promise<void> {
    return request<void>(`/repos/${encodeURIComponent(repository_id)}/goal`, { method: "DELETE" });
}

export function ignite_commander(repository_id: string, brief?: string): Promise<Ignited> {
    return request<Ignited>(`/repos/${encodeURIComponent(repository_id)}/commander`, {
        method: "POST",
        body: JSON.stringify(brief ? { brief } : {}),
    });
}

/// Merge a worktree's pull request and finish the card with it.
///
/// A person's call: it puts code in front of everyone and no button takes it
/// back.
export function merge_worktree(repository_id: string, name: string, task_id: string): Promise<Task> {
    return request<Task>(
        `/repos/${encodeURIComponent(repository_id)}/worktrees/${encodeURIComponent(name)}/merge`,
        { method: "POST", body: JSON.stringify({ task_id }) },
    );
}

export function begin_project(beginning: Beginning): Promise<Begun> {
    return request<Begun>("/start", { method: "POST", body: JSON.stringify(beginning) });
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
export type Presence = "idle" | "working" | "waiting" | "done" | "attention";

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
    /// What the commander decided: the model it runs on, what its pane is called
    /// and the colour the crew knows it by. Null means nobody has decided.
    model: string | null;
    title: string | null;
    colour: string | null;
    /// How much this agent may do without asking; null means its role's default.
    permissions: string | null;
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
    at?: number;
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

/// Where a memory belongs, in the vault's own words: "shared",
/// "workspace:<id>", "project:<workspace>/<project>". It is the folder the note
/// is written into, so the scope is visible on disk.
export type MemoryScope = string;

export interface Memory {
    /// The note's slug in the vault, e.g. `atolye/svc-demo/memory/the-dev-server-reads-port`.
    id: string;
    text: string;
    /// The folder it lives in, without the `memory/` leaf.
    scope: string;
    proposed_by: string;
    /// When it was written down, in seconds.
    written_at: number;
    /// The memory this one replaces, by slug, when it replaces one.
    supersedes?: string | null;
    /// Approved once, then taken back out. Kept, and not told to anyone.
    retired?: boolean;
    approved: boolean;
    masked: boolean;
}

/// What answering a memory did: the memory, and the one it took out of the
/// crew's brief by superseding it.
export interface Answered extends Memory {
    replaced: string | null;
}

export function list_memories(): Promise<Memory[]> {
    return request<Memory[]>("/memories");
}

export function propose_memory(
    text: string,
    scope: MemoryScope,
    proposed_by: string,
): Promise<Memory> {
    return request<Memory>("/memories", {
        method: "POST",
        body: JSON.stringify({ text, scope, proposed_by }),
    });
}

/// Say yes or no to a memory. It is addressed by its note's slug, which has
/// slashes in it, so the slug travels in the body rather than the path.
export function answer_memory(slug: string, approved: boolean): Promise<Answered> {
    return request<Answered>("/memories/answer", {
        method: "POST",
        body: JSON.stringify({ slug, approved }),
    });
}

export function forget_memory(slug: string): Promise<void> {
    return request<void>(`/memories/${slug}`, { method: "DELETE" });
}

export interface Recalled {
    memory: Memory;
    score: number;
    lexical: number;
    semantic: number;
}

export interface EmbedderSettings {
    endpoint: string | null;
    model: string;
    min_similarity: number;
}

export interface EmbedderReport {
    settings: EmbedderSettings;
    reachable: boolean;
    dimensions: number;
    detail: string;
}

export function search_memories(query: string, limit = 8): Promise<Recalled[]> {
    return request<Recalled[]>(`/memories/search?q=${encodeURIComponent(query)}&limit=${limit}`);
}

export function read_embedder(): Promise<EmbedderReport> {
    return request<EmbedderReport>("/memories/embedder");
}

export function set_embedder(settings: EmbedderSettings): Promise<EmbedderReport> {
    return request<EmbedderReport>("/memories/embedder", {
        method: "POST",
        body: JSON.stringify(settings),
    });
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
    at?: number;
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

/// What the commander decided about an agent. Fields left out stay as they were.
export interface Note {
    /// null for an ordinary note; false for a memory waiting on you; true for
    /// one the crew may be told without looking it up.
    approved?: boolean | null;
    slug: string;
    title: string;
    tags: string[];
    written_by: string;
    written_at: number;
    body: string;
    links: string[];
    backlinks: string[];
}

export interface Notice {
    id: number;
    kind: "waiting" | "finished" | "trouble" | "word";
    text: string;
    workspace_id: string | null;
    repository_id: string | null;
    agent_id: string | null;
    opens: string | null;
    at: number;
    seen: boolean;
}

export interface NoticeReport {
    notices: Notice[];
    unseen: number;
    loud: boolean;
}

export function read_notices(limit = 40): Promise<NoticeReport> {
    return request<NoticeReport>(`/notices?limit=${limit}`);
}

export function mark_notices_seen(ids: number[] = []): Promise<void> {
    return request<void>("/notices", { method: "POST", body: JSON.stringify({ ids }) });
}

export interface VaultReport {
    path: string;
    notes: number;
}

export function read_vault(): Promise<VaultReport> {
    return request<VaultReport>("/vault");
}

export function list_notes(query?: string, limit = 40): Promise<Note[]> {
    const search = new URLSearchParams({ limit: String(limit) });
    if (query && query.trim()) {
        search.set("q", query.trim());
    }
    return request<Note[]>(`/notes?${search.toString()}`);
}

export function read_note(slug: string): Promise<Note> {
    return request<Note>(`/notes/${encodeURIComponent(slug)}`);
}

export function write_note(draft: {
    title: string;
    body: string;
    tags?: string[];
    written_by?: string;
}): Promise<Note> {
    return request<Note>("/notes", { method: "POST", body: JSON.stringify(draft) });
}

export function forget_note(slug: string): Promise<void> {
    return request<void>(`/notes/${encodeURIComponent(slug)}`, { method: "DELETE" });
}

export function shape_agent(
    id: string,
    wanted: { model?: string; title?: string; colour?: string; permissions?: string },
): Promise<Agent> {
    return request<Agent>(`/agents/${encodeURIComponent(id)}`, {
        method: "POST",
        body: JSON.stringify(wanted),
    });
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

/// What an agent still has in hand. Read before it is let go, because
/// dismissing cannot be undone and the agent is what knows where its work got to.
export interface Holdings {
    cards: Array<{ id: string; title: string; column: Column }>;
    pane_running: boolean;
    uncommitted: number;
    unpushed: number;
    worktree?: string;
    empty_handed: boolean;
}

export function read_holdings(id: string): Promise<Holdings> {
    return request<Holdings>(`/agents/${id}/holdings`);
}

export function dismiss_agent(id: string, anyway = false): Promise<void> {
    return request<void>(`/agents/${id}?anyway=${anyway}`, { method: "DELETE" });
}

export type Column = "backlog" | "assigned" | "working" | "review" | "ready" | "done";

export interface Evidence {
    kind: "commit" | "diff" | "pull_request" | "note" | "finished" | string;
    [key: string]: unknown;
}

/// A piece of evidence and who put it there. Entries written before anyone
/// signed their work come back as `someone` with no time on them.
export interface Entry {
    what: Evidence;
    by: string;
    at: number;
}

export interface Task {
    /// When the card was written. Zero for cards from before this was recorded.
    at?: number;
    id: string;
    title: string;
    body: string;
    column: Column;
    repository_id: string;
    assignee: string | null;
    worktree: string | null;
    branch: string | null;
    evidence: Entry[];
    /// Where it sits in its column, smallest first.
    position?: number;
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

/// Drop a card into a column, above `before` — or at the bottom without one.
export function place_task(id: string, column: Column, before?: string): Promise<Task> {
    return request<Task>(`/tasks/${encodeURIComponent(id)}/place`, {
        method: "POST",
        body: JSON.stringify(before ? { column, before } : { column }),
    });
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

export interface Listing {
    root: string;
    path: string;
    entries: { name: string; kind: "dir" | "file"; size: number }[];
}

export interface FileText {
    path: string;
    text: string;
    bytes: number;
    truncated: boolean;
}

/// What is in a folder of a project, or of one of its worktrees.
///
/// A project's folder and the folder an agent works in are two different places,
/// so which one is being read is always said out loud: no worktree means the
/// project's own checkout.
export function list_files(repository_id: string, path: string, worktree?: string | null): Promise<Listing> {
    const query = new URLSearchParams({ path });
    if (worktree) {
        query.set("worktree", worktree);
    }
    return request<Listing>(`/repos/${repository_id}/files?${query.toString()}`);
}

export function read_file(repository_id: string, path: string, worktree?: string | null): Promise<FileText> {
    const query = new URLSearchParams({ path });
    if (worktree) {
        query.set("worktree", worktree);
    }
    return request<FileText>(`/repos/${repository_id}/file?${query.toString()}`);
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

export type StepState = "waiting" | "assigned" | "done" | "blocked";

export interface PlanStep {
    id: string;
    title: string;
    brief: string;
    needs: string[];
    task_id: string | null;
    note: string | null;
    state: StepState;
}

export interface Plan {
    id: string;
    goal: string;
    repository_id: string;
    created_by: string;
    state: "running" | "done" | "abandoned";
    steps: PlanStep[];
}

export interface ReadyStep {
    plan_id: string;
    goal: string;
    repository_id: string;
    step: PlanStep;
}

export interface Watch {
    id: string;
    plan_id: string;
    step_id: string;
    task_id: string;
    agent_id: string;
    session_id: string;
    delivered: boolean;
    resends: number;
    state: "working" | "settled" | "abandoned";
    reason: string | null;
    told_leader: boolean;
    wake_attempts: number;
    reaped: boolean;
}

export function list_plans(): Promise<Plan[]> {
    return request<Plan[]>("/plans");
}

export function ready_steps(): Promise<ReadyStep[]> {
    return request<ReadyStep[]>("/plans/ready");
}

export function mark_step(
    plan_id: string,
    step_id: string,
    state: StepState,
    note?: string,
): Promise<Plan> {
    return request<Plan>(`/plans/${encodeURIComponent(plan_id)}/steps/${encodeURIComponent(step_id)}`, {
        method: "POST",
        body: JSON.stringify({ state, note }),
    });
}

export function supervisor_watches(): Promise<Watch[]> {
    return request<Watch[]>("/supervisor");
}

export interface PaneView {
    holder: string;
    readable: boolean;
}

export function list_windows(): Promise<Record<string, PaneView>> {
    return request<Record<string, PaneView>>("/ui/windows");
}

export function set_window(
    session_id: string,
    change: { holder?: string; readable?: boolean },
): Promise<Record<string, PaneView>> {
    return request<Record<string, PaneView>>("/ui/windows", {
        method: "POST",
        body: JSON.stringify({ session_id, ...change }),
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
    /// When it was asked, and when it was answered. Zero where it predates the
    /// app recording either.
    at?: number;
    answered_at?: number;
}

export function list_approvals(): Promise<Approval[]> {
    return request<Approval[]>("/approvals");
}
