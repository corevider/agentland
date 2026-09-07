/// Where a shell would open, read off the folder a pane is already in.
///
/// A pane's working folder is inside one of the project's worktrees or its
/// main checkout; the longest path that contains it says which, so a worktree
/// under the checkout's folder wins over the checkout itself.
export interface Standing {
    repository_id: string;
    worktree: string | null;
    path: string;
}

export function standing_of(
    cwd: string | null | undefined,
    repos: { id: string; primary_path: string }[],
    worktrees: { repository_id: string; name: string; path: string }[],
): Standing | null {
    if (!cwd) {
        return null;
    }

    const places: Standing[] = [
        ...worktrees.map((held) => ({ repository_id: held.repository_id, worktree: held.name, path: held.path })),
        ...repos.map((held) => ({ repository_id: held.id, worktree: null, path: held.primary_path })),
    ];

    const inside = (folder: string) => cwd === folder || cwd.startsWith(folder.endsWith("/") ? folder : `${folder}/`);

    return places.filter((place) => inside(place.path)).sort((a, b) => b.path.length - a.path.length)[0] ?? null;
}

/// The last folder of a path, for a label when nothing better is known.
export function folder_name(path: string): string {
    return path.replace(/\/+$/, "").split("/").pop() || path;
}

export interface Place {
    path: string;
    label: string;
}

/// The folders a CLI could open in, for one project: its main checkout first,
/// then every worktree cut from it.
///
/// A place that is gone from disk is left out rather than offered and refused.
/// A pane opened at a folder that is not there does not fail — it opens in the
/// home folder instead, which is the one place an engine should never start.
export function places_in(
    known: {
        repos: { id: string; primary_path: string; default_branch: string; missing?: boolean }[];
        trees: { repository_id: string; name: string; path: string; branch: string; missing?: boolean }[];
    },
    repository_id: string,
): Place[] {
    const repo = known.repos.find((held) => held.id === repository_id);
    const places: Place[] = [];

    if (repo && !repo.missing) {
        places.push({ path: repo.primary_path, label: `main checkout · ${repo.default_branch}` });
    }

    for (const tree of known.trees) {
        if (tree.repository_id === repository_id && !tree.missing) {
            places.push({ path: tree.path, label: `${tree.name} · ${tree.branch}` });
        }
    }

    return places;
}

/// The place a form should be holding, given what it holds and what it offers.
///
/// A `<select>` whose value matches none of its options renders the first one
/// and keeps the other, so the form reads as one place while carrying another.
/// That shipped: the place shown was a live worktree, the place held was a
/// checkout deleted from disk, and starting there failed on a path the person
/// had never chosen. Anything not on offer becomes the first thing that is.
export function settled_place(wanted: string, offered: Place[]): string {
    if (offered.length === 0) {
        return wanted;
    }

    return offered.some((place) => place.path === wanted) ? wanted : offered[0].path;
}

/// What to call the folder a pane is standing in, in the few characters a
/// footer has.
///
/// A worktree is named with its project, because "ada-tree" alone says nothing
/// about which project it was cut from once two of them have one. A main
/// checkout is just the project. Anywhere else is the folder's own name — a
/// shell can be opened outside every project, and that is worth saying rather
/// than leaving blank.
export function place_label(
    cwd: string | null | undefined,
    repos: { id: string; primary_path: string }[],
    worktrees: { repository_id: string; name: string; path: string }[],
): string | null {
    const here = standing_of(cwd, repos, worktrees);

    if (!here) {
        return cwd ? folder_name(cwd) : null;
    }

    return here.worktree ? `${here.repository_id}/${here.worktree}` : here.repository_id;
}

/// Whether what a pane is called already says who is working in it.
///
/// The crew's name is put beside a pane's own only when the pane would
/// otherwise not say it. Asking who set the name is the wrong question — the
/// live commander names its own pane "X · commander", and a badge repeating
/// that said the name and the role twice. Asking whether the name is in there
/// holds however it got there.
///
/// Matched on whole words, so "Xavier" does not count as having named "X".
export function names_the_agent(label: string | null | undefined, name: string | null | undefined): boolean {
    if (!label || !name) {
        return false;
    }

    const loose = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    return new RegExp(`(^|\\W)${loose}($|\\W)`, "i").test(label);
}

/// What the core is running that the panel is not showing.
///
/// Hiding an agent's pane leaves the agent working, which is the point of
/// hiding rather than stopping it — but it also left no way back to it short of
/// remembering it existed. A pane held by a window of its own is not out of
/// sight; it is on screen somewhere else, and offering to open it again in the
/// grid would be offering it twice.
export function out_of_sight<T extends { id: string }>(
    running: T[],
    shown: { id: string }[],
    holder_of: (id: string) => string | undefined,
): T[] {
    const on_screen = new Set(shown.map((entry) => entry.id));

    return running.filter((entry) => !on_screen.has(entry.id) && !holder_of(entry.id));
}
