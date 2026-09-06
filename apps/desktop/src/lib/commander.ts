/// Whether the commander's pane is worth clearing.
///
/// A commander that has seen a plan through carries the whole of it in its
/// context, and the next goal pays for every token of it. Once nothing it
/// held is still open, /clear costs nothing: every brief that follows is
/// composed again with its identity, the crew, what the project remembers
/// and its mail, and the house rules ride on a flag that a clear leaves alone.
export interface CommanderLoad {
    has_pane: boolean;
    running_plans: number;
    open_cards: number;
    finished_anything: boolean;
}

export function clear_is_recommended(load: CommanderLoad): boolean {
    return load.has_pane && load.running_plans === 0 && load.open_cards === 0 && load.finished_anything;
}

/// Who commands what, in a crew that has both kinds of commander.
///
/// A chief belongs to a workspace and a commander belongs to a project, and
/// both answer to the name X. Finding either by role alone shows one
/// workspace's chief while you are standing in another — the same wrong that
/// reading the first commander in the crew was.
export interface Commanding {
    role: string;
    repository_id: string;
    workspace_id: string | null;
}

export function chief_of<T extends Commanding>(crew: T[], workspace_id: string | null): T | undefined {
    if (!workspace_id) {
        return undefined;
    }

    return crew.find((agent) => agent.role === "chief" && agent.workspace_id === workspace_id);
}

export function commander_of<T extends Commanding>(crew: T[], repository_id: string): T | undefined {
    return crew.find((agent) => agent.role === "commander" && agent.repository_id === repository_id);
}
