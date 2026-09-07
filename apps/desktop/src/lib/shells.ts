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
