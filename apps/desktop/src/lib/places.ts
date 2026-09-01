import type { Agent, Repository, Workspace, WorktreeStatus } from "@/lib/core";

export type PlaceKind = "workspace" | "project" | "worktree" | "agent";

export interface Place {
    kind: PlaceKind;
    id: string;
    name: string;
    /// The line under the name: where on disk, which branch, whose worktree.
    detail: string;
    /// The short name the crew uses, when the shown name is something longer —
    /// an agent titled "X · commander" is still called X by everyone.
    alias?: string;
    /// Which workspace has to be active for this place to be on screen.
    workspace_id: string | null;
    /// The workspace's name, so someone can type "demos ada" and mean it. Not
    /// shown on the row — the row already says what it is — but searched.
    workspace_name: string | null;
    repository_id: string | null;
    worktree: string | null;
    agent_id: string | null;
}

export interface World {
    workspaces: Workspace[];
    active_workspace: string | null;
    repositories: Repository[];
    worktrees: WorktreeStatus[];
    agents: Agent[];
}

/// The home folder, read from the paths themselves.
///
/// The app cannot ask the operating system where home is, and the crew's paths
/// are the only evidence to hand: `/home/someone/...` on Linux, `/Users/...` on
/// a Mac. Nothing matches means nothing is shortened, which is only verbose.
export function home_from(paths: string[]): string {
    for (const path of paths) {
        const match = /^(\/(?:home|Users)\/[^/]+)\//.exec(path);
        if (match) {
            return match[1];
        }
    }

    return "";
}

function shorten(path: string, home: string): string {
    return home && path.startsWith(home) ? `~${path.slice(home.length)}` : path;
}

function workspace_holding(workspaces: Workspace[], repository_id: string): Workspace | null {
    return workspaces.find((workspace) => workspace.repository_ids.includes(repository_id)) ?? null;
}

/// Every place a person can go, in the order they think of them.
///
/// A workspace is a set of projects, a project is a folder, and the folder a
/// worktree sits in is somewhere else entirely — so each place carries the path
/// or branch that tells them apart, and the workspace that has to be active for
/// it to be visible.
export function places_from(world: World, home = ""): Place[] {
    const places: Place[] = [];

    for (const workspace of world.workspaces) {
        const count = workspace.repository_ids.length;
        places.push({
            kind: "workspace",
            id: `workspace:${workspace.id}`,
            name: workspace.name,
            detail: count === 1 ? "1 project" : `${count} projects`,
            workspace_id: workspace.id,
            workspace_name: workspace.name,
            repository_id: null,
            worktree: null,
            agent_id: null,
        });
    }

    for (const repository of world.repositories) {
        places.push({
            kind: "project",
            id: `project:${repository.id}`,
            name: repository.name,
            detail: shorten(repository.primary_path, home),
            workspace_id: workspace_holding(world.workspaces, repository.id)?.id ?? null,
            workspace_name: workspace_holding(world.workspaces, repository.id)?.name ?? null,
            repository_id: repository.id,
            worktree: null,
            agent_id: null,
        });
    }

    for (const worktree of world.worktrees) {
        const project = world.repositories.find((repository) => repository.id === worktree.repository_id);
        places.push({
            kind: "worktree",
            id: `worktree:${worktree.repository_id}/${worktree.name}`,
            name: worktree.name,
            detail: `${project?.name ?? worktree.repository_id} · ${worktree.branch} · ${shorten(worktree.path, home)}`,
            workspace_id: workspace_holding(world.workspaces, worktree.repository_id)?.id ?? null,
            workspace_name: workspace_holding(world.workspaces, worktree.repository_id)?.name ?? null,
            repository_id: worktree.repository_id,
            worktree: worktree.name,
            agent_id: null,
        });
    }

    for (const agent of world.agents) {
        const project = world.repositories.find((repository) => repository.id === agent.repository_id);
        places.push({
            kind: "agent",
            id: `agent:${agent.id}`,
            name: agent.title || agent.name,
            alias: agent.name,
            detail: `${agent.name} · ${agent.role} · ${project?.name ?? agent.repository_id} · ${agent.worktree}`,
            workspace_id: workspace_holding(world.workspaces, agent.repository_id)?.id ?? null,
            workspace_name: workspace_holding(world.workspaces, agent.repository_id)?.name ?? null,
            repository_id: agent.repository_id,
            worktree: agent.worktree,
            agent_id: agent.id,
        });
    }

    return places;
}

function hit(name: string, alias: string, detail: string, term: string): number {
    if (name === term || alias === term) {
        return 100;
    }
    if (name.startsWith(term) || alias.startsWith(term)) {
        return 80;
    }
    if (name.includes(term)) {
        return 60;
    }
    if (detail.includes(term)) {
        return 30;
    }

    return 0;
}

/// How well a place answers what was typed. Zero means it does not.
///
/// Someone typing "ada" wants the agent, not every worktree that mentions it in
/// a path, so a hit on the name outranks a hit on the detail line, and a place
/// whose name starts with the query outranks one that merely contains it.
///
/// People also type the way they think: "agentland x" is the project and then
/// the agent in it, and neither half is a substring of the other. So every word
/// has to land somewhere — name or detail — and the score is how well they
/// landed on average.
export function score(place: Place, query: string): number {
    const terms = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
    if (terms.length === 0) {
        return 1;
    }

    const name = place.name.toLowerCase();
    const alias = (place.alias ?? place.name).toLowerCase();
    const detail = `${place.detail} ${place.workspace_name ?? ""}`.toLowerCase();

    let total = 0;
    for (const term of terms) {
        const found = hit(name, alias, detail, term);
        if (found === 0) {
            return 0;
        }
        total += found;
    }

    return total / terms.length;
}

/// Someone who has typed something is usually after a person, and the crew's
/// names are the ones they type.
const RANK: Record<PlaceKind, number> = {
    agent: 0,
    worktree: 1,
    project: 2,
    workspace: 3,
};

/// Someone who has typed nothing is browsing, and browsing goes the other way:
/// the widest place first, so the list reads like the structure it belongs to.
const BROWSE: Record<PlaceKind, number> = {
    workspace: 0,
    project: 1,
    worktree: 2,
    agent: 3,
};

/// The places worth showing for what was typed, best first.
export function search_places(places: Place[], query: string, most = 12): Place[] {
    const order = query.trim() ? RANK : BROWSE;

    return places
        .map((place) => ({ place, hit: score(place, query) }))
        .filter((held) => held.hit > 0)
        .sort((left, right) => right.hit - left.hit || order[left.place.kind] - order[right.place.kind])
        .slice(0, most)
        .map((held) => held.place);
}

/// Whether going here means activating a different workspace first.
///
/// With no workspace active every project is already on screen, so nothing has
/// to be switched — saying otherwise on every row taught people to ignore it.
export function needs_switch(place: Place, active_workspace: string | null): boolean {
    if (place.kind === "workspace") {
        return place.workspace_id !== active_workspace;
    }

    if (active_workspace === null) {
        return false;
    }

    return place.workspace_id !== null && place.workspace_id !== active_workspace;
}

/// The trail shown in the header: where the person is standing right now.
export function trail(world: World, repository_id: string | null, worktree: string | null, home = ""): string[] {
    const crumbs: string[] = [];
    const workspace = world.workspaces.find((held) => held.id === world.active_workspace);
    crumbs.push(workspace?.name ?? "everything");

    const project = world.repositories.find((held) => held.id === repository_id);
    if (project) {
        crumbs.push(project.name);
    }

    const tree = world.worktrees.find(
        (held) => held.repository_id === repository_id && held.name === worktree,
    );
    if (tree) {
        crumbs.push(`${tree.name} · ${tree.branch}`);
    } else if (project) {
        crumbs.push(shorten(project.primary_path, home));
    }

    return crumbs;
}
