import type { Repository } from "@/lib/core";

/// Somewhere an agent can be put to work.
///
/// Every project offers a worktree of its own, whatever it already holds: two
/// agents standing in one worktree share a branch and fight over the same
/// lines, so a new hire wanting its own place is the ordinary case, not the
/// case where the project happens to be empty. The worktrees that exist are
/// offered under it, for the hire that is meant to join work already going on.
export interface Target {
    repository_id: string;
    /// The worktree to hire into, or "" when it is still to be cut.
    worktree: string;
    label: string;
}

const MOST_TRIES = 99;

export function target_value(target: Target): string {
    return `${target.repository_id}/${target.worktree}`;
}

export function hiring_targets(
    repos: Repository[],
    worktrees: Array<{ repository_id: string; name: string }>,
): Target[] {
    return repos.flatMap((repo) => [
        {
            repository_id: repo.id,
            worktree: "",
            label: `${repo.id} · a worktree of its own`,
        },
        ...worktrees
            .filter((entry) => entry.repository_id === repo.id)
            .map((entry) => ({
                repository_id: repo.id,
                worktree: entry.name,
                label: `${repo.id}/${entry.name}`,
            })),
    ]);
}

function slug(value: string): string {
    let out = "";

    for (const character of value) {
        if (/[a-zA-Z0-9]/.test(character)) {
            out += character.toLowerCase();
        } else if (out.length > 0 && !out.endsWith("-")) {
            out += "-";
        }
    }

    return out.replace(/-+$/, "");
}

/// What the worktree cut for a new agent is called: the agent's own name, so
/// the branch, the folder and the person answering for them all read the same.
///
/// A name already standing in that project is not a clash to report — an agent
/// dismissed leaves its worktree behind, and hiring another of the same name is
/// a thing people do. It takes the next free one rather than failing.
export function worktree_for(name: string, taken: string[] = []): string | null {
    const base = slug(name);
    if (base.length === 0) {
        return null;
    }

    if (!taken.includes(base)) {
        return base;
    }

    for (let number = 2; number <= MOST_TRIES; number += 1) {
        if (!taken.includes(`${base}-${number}`)) {
            return `${base}-${number}`;
        }
    }

    return `${base}-${MOST_TRIES}`;
}
