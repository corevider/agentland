import type { Repository } from "@/lib/core";

/// Somewhere an agent can be put to work.
///
/// A worktree that exists is one. A project that has none yet is also one: an
/// agent owns a worktree, so the first hire into a project cuts it rather than
/// leaving a person in front of an empty list with nothing to pick.
export interface Target {
    repository_id: string;
    /// The worktree to hire into, or "" when it is still to be cut.
    worktree: string;
    label: string;
}

export function target_value(target: Target): string {
    return `${target.repository_id}/${target.worktree}`;
}

export function hiring_targets(
    repos: Repository[],
    worktrees: Array<{ repository_id: string; name: string }>,
): Target[] {
    const held = worktrees.map((entry) => ({
        repository_id: entry.repository_id,
        worktree: entry.name,
        label: `${entry.repository_id}/${entry.name}`,
    }));

    const without = repos
        .filter((repo) => !worktrees.some((entry) => entry.repository_id === repo.id))
        .map((repo) => ({
            repository_id: repo.id,
            worktree: "",
            label: `${repo.id} · a worktree of its own`,
        }));

    return [...held, ...without];
}

/// What the worktree cut for a new agent is called: the agent's own name, so
/// the branch, the folder and the person answering for them all read the same.
export function worktree_for(name: string): string | null {
    let out = "";

    for (const character of name) {
        if (/[a-zA-Z0-9]/.test(character)) {
            out += character.toLowerCase();
        } else if (out.length > 0 && !out.endsWith("-")) {
            out += "-";
        }
    }

    const trimmed = out.replace(/-+$/, "");
    return trimmed.length > 0 ? trimmed : null;
}
