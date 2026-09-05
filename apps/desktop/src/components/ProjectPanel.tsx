import { useCallback, useEffect, useMemo, useState } from "react";

import {
    list_files,
    list_repos,
    list_worktrees,
    read_file,
    review_worktree,
    review_project,
    type FileText,
    type Listing,
    type Repository,
    type Review,
    type WorktreeStatus,
} from "@/lib/core";
import { use_poll } from "@/lib/poll";
import {
    crumbs_of,
    hunks_of,
    is_probably_text,
    join_path,
    line_kind,
    parent_of,
    size_word,
    sort_entries,
} from "@/lib/tree";

const DIFF_TINT: Record<string, string> = {
    added: "text-palm",
    removed: "text-coral",
    meta: "text-shade",
    same: "text-driftwood",
};

interface Props {
    active: boolean;
    repositories: string[] | null;
    going: { repository_id: string | null; worktree: string | null; at: number } | null;
}

/// A project's files and its git, side by side.
///
/// The folder a project lives in and the folder an agent works in are different
/// places on disk, and the difference is what makes a change hard to find. So
/// the checkout being read is always named at the top, and switching between the
/// project's own folder and any agent's worktree is one click.
export function ProjectPanel({ active, repositories, going }: Props) {
    const [repos, set_repos] = useState<Repository[]>([]);
    const [repository_id, set_repository] = useState<string | null>(null);
    const [worktrees, set_worktrees] = useState<WorktreeStatus[]>([]);
    const [worktree, set_worktree] = useState<string | null>(null);
    const [path, set_path] = useState("");
    const [listing, set_listing] = useState<Listing | null>(null);
    const [opened, set_opened] = useState<FileText | null>(null);
    const [review, set_review] = useState<Review | null>(null);
    const [showing, set_showing] = useState<"files" | "git">("files");
    const [error, set_error] = useState<string | null>(null);

    const shown = useMemo(
        () => (repositories ? repos.filter((repo) => repositories.includes(repo.id)) : repos),
        [repos, repositories],
    );

    useEffect(() => {
        list_repos().then(set_repos).catch((cause) => set_error(String(cause)));
    }, []);

    useEffect(() => {
        if (!repository_id && shown.length > 0) {
            set_repository(shown[0].id);
        }
    }, [shown, repository_id]);

    useEffect(() => {
        if (!repository_id) {
            return;
        }

        set_path("");
        set_opened(null);
        set_worktree(null);
        list_worktrees(repository_id).then(set_worktrees).catch(() => set_worktrees([]));
    }, [repository_id]);

    // The jumper says where to look; arriving here should already be there.
    useEffect(() => {
        if (!going?.repository_id) {
            return;
        }

        set_repository(going.repository_id);
        set_worktree(going.worktree);
        set_path("");
        set_opened(null);
    }, [going?.at, going?.repository_id, going?.worktree]);

    const refresh = useCallback(() => {
        if (!repository_id) {
            return;
        }

        list_files(repository_id, path, worktree)
            .then((held) => {
                set_listing(held);
                set_error(null);
            })
            .catch((cause) => set_error(cause instanceof Error ? cause.message : String(cause)));

        const reading = worktree
            ? review_worktree(repository_id, worktree)
            : review_project(repository_id);
        reading.then(set_review).catch(() => set_review(null));
    }, [repository_id, path, worktree]);

    useEffect(refresh, [refresh]);
    use_poll(refresh, 8000, active);

    const open = useCallback(
        (name: string, kind: "dir" | "file") => {
            if (!repository_id) {
                return;
            }

            if (kind === "dir") {
                set_opened(null);
                set_path(join_path(path, name));
                return;
            }

            if (!is_probably_text(name)) {
                set_opened({ path: join_path(path, name), text: "", bytes: 0, truncated: false });
                return;
            }

            read_file(repository_id, join_path(path, name), worktree)
                .then(set_opened)
                .catch((cause) => set_error(cause instanceof Error ? cause.message : String(cause)));
        },
        [repository_id, path, worktree],
    );

    const entries = sort_entries(listing?.entries ?? []);
    const patch = hunks_of(review?.patch ?? "");

    return (
        <div className="flex h-full min-h-0 min-w-0 flex-1 flex-col gap-2 p-2.5">
            <header className="flex flex-wrap items-center gap-2">
                <select
                    className="rounded-md border border-reef bg-lagoon-deep px-2 py-1 font-mono text-[11px] text-linen"
                    value={repository_id ?? ""}
                    onChange={(event) => set_repository(event.target.value || null)}
                >
                    {shown.map((repo) => (
                        <option key={repo.id} value={repo.id}>
                            {repo.name}
                        </option>
                    ))}
                </select>

                <select
                    className="rounded-md border border-reef bg-lagoon-deep px-2 py-1 font-mono text-[11px] text-linen"
                    value={worktree ?? ""}
                    title="the project's own folder, or the folder an agent works in"
                    onChange={(event) => {
                        set_worktree(event.target.value || null);
                        set_path("");
                        set_opened(null);
                    }}
                >
                    <option value="">the project itself</option>
                    {worktrees.map((tree) => (
                        <option key={tree.name} value={tree.name}>
                            {tree.name} · {tree.branch}
                            {tree.dirty_files > 0 ? ` · ${tree.dirty_files} dirty` : ""}
                        </option>
                    ))}
                </select>

                <div className="flex rounded-md border border-reef">
                    {(["files", "git"] as const).map((tab) => (
                        <button
                            key={tab}
                            className={`px-2 py-1 font-mono text-[11px] ${
                                showing === tab ? "bg-shallow text-linen" : "text-shell hover:text-linen"
                            }`}
                            onClick={() => set_showing(tab)}
                        >
                            {tab}
                        </button>
                    ))}
                </div>

                {listing ? (
                    <span className="cursor-text select-text font-mono text-[10px] text-shade" title="on disk">
                        {listing.root}
                    </span>
                ) : null}
            </header>

            {error ? (
                <div className="rounded-md border border-coral px-2 py-1 font-mono text-[11px] text-coral">
                    {error}
                </div>
            ) : null}

            {showing === "files" ? (
                <div className="flex min-h-0 flex-1 gap-2">
                    <section className="flex min-h-0 w-64 shrink-0 flex-col gap-1 overflow-y-auto rounded-md border border-reef bg-lagoon-deep p-1.5">
                        <div className="mb-0.5 flex flex-wrap items-center gap-0.5 border-b border-reef/70 pb-1 font-mono text-[10px] text-shade">
                            {crumbs_of(path).map((crumb) => (
                                <button
                                    key={crumb.path}
                                    className="hover:text-turquoise"
                                    onClick={() => {
                                        set_path(crumb.path);
                                        set_opened(null);
                                    }}
                                >
                                    {crumb.name}/
                                </button>
                            ))}
                        </div>

                        {path ? (
                            <button
                                className="text-left font-mono text-[11px] text-shell hover:text-turquoise"
                                onClick={() => {
                                    set_path(parent_of(path));
                                    set_opened(null);
                                }}
                            >
                                ..
                            </button>
                        ) : null}

                        {entries.map((entry) => (
                            <button
                                key={entry.name}
                                className={`flex items-baseline gap-1.5 text-left font-mono text-[11px] hover:text-turquoise ${
                                    opened?.path === join_path(path, entry.name) ? "text-turquoise" : "text-driftwood"
                                }`}
                                onClick={() => open(entry.name, entry.kind)}
                            >
                                <span className="truncate">
                                    {entry.kind === "dir" ? `${entry.name}/` : entry.name}
                                </span>
                                {entry.kind === "file" ? (
                                    <span className="ml-auto shrink-0 text-[9px] text-shade">
                                        {size_word(entry.size)}
                                    </span>
                                ) : null}
                            </button>
                        ))}

                        {entries.length === 0 ? (
                            <p className="font-mono text-[10px] text-shade">nothing here</p>
                        ) : null}
                    </section>

                    <section className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-md border border-reef bg-lagoon-deep">
                        {opened ? (
                            <>
                                <div className="flex items-baseline gap-2 border-b border-reef px-2 py-1">
                                    <span className="font-mono text-[11px] text-linen">{opened.path}</span>
                                    <span className="font-mono text-[9px] text-shade">
                                        {size_word(opened.bytes)}
                                        {opened.truncated ? " · shown in part" : ""}
                                    </span>
                                </div>
                                <pre className="min-h-0 flex-1 overflow-auto p-2 font-mono text-[11px] leading-relaxed text-driftwood">
                                    {opened.text || "not text — nothing to read here"}
                                </pre>
                            </>
                        ) : (
                            <p className="p-2 font-mono text-[10px] text-shade">
                                Pick a file to read it. Nothing here is edited — this is the crew's work as it
                                stands on disk.
                            </p>
                        )}
                    </section>
                </div>
            ) : (
                <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto">
                    {!worktree ? (
                        <p className="font-mono text-[10px] text-shade">
                            The project's own checkout. Pick a worktree above to read an agent's branch
                            against it instead.
                        </p>
                    ) : null}

                    {review ? (
                        <>
                            <div className="flex flex-wrap items-baseline gap-2 rounded-md border border-reef bg-lagoon-deep px-2 py-1 font-mono text-[11px]">
                                <span className="text-linen">{review.branch}</span>
                                <span className="text-shade">against {review.base}</span>
                                <span className="text-palm">+{review.insertions}</span>
                                <span className="text-coral">−{review.deletions}</span>
                                <span className="text-shade">
                                    {review.files} file{review.files === 1 ? "" : "s"}
                                </span>
                                {review.uncommitted ? <span className="text-sun">uncommitted work</span> : null}
                            </div>

                            {review.commits.length > 0 ? (
                                <section className="rounded-md border border-reef bg-lagoon-deep p-1.5">
                                    {review.commits.map((commit) => (
                                        <div key={commit.sha} className="flex gap-2 font-mono text-[11px]">
                                            <span className="text-shade">{commit.sha}</span>
                                            <span className="truncate text-driftwood">{commit.subject}</span>
                                        </div>
                                    ))}
                                </section>
                            ) : null}

                            {review.untracked.length > 0 ? (
                                <section className="rounded-md border border-reef bg-lagoon-deep p-1.5 font-mono text-[11px] text-sun">
                                    new, not yet added: {review.untracked.join(", ")}
                                </section>
                            ) : null}

                            {patch.map((hunk) => (
                                <section
                                    key={hunk.file}
                                    className="overflow-hidden rounded-md border border-reef bg-lagoon-deep"
                                >
                                    <div className="border-b border-reef px-2 py-1 font-mono text-[11px] text-linen">
                                        {hunk.file}
                                    </div>
                                    <pre className="overflow-x-auto p-1.5 font-mono text-[10px] leading-relaxed">
                                        {hunk.lines.map((line, index) => (
                                            <div key={index} className={DIFF_TINT[line_kind(line)]}>
                                                {line || " "}
                                            </div>
                                        ))}
                                    </pre>
                                </section>
                            ))}

                            {patch.length === 0 ? (
                                <p className="font-mono text-[10px] text-shade">
                                    nothing changed against {review.base} yet
                                </p>
                            ) : null}
                        </>
                    ) : null}
                </div>
            )}
        </div>
    );
}
