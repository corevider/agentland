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
