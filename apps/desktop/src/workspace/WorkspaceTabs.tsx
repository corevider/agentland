import { useCallback, useEffect, useRef, useState } from "react";

import {
    activate_workspace,
    create_workspace,
    list_repos,
    list_workspaces,
    remove_workspace,
    set_workspace_repos,
    type Repository,
    type Workspace,
} from "@/lib/core";

interface Props {
    /// Changes when someone else activated a workspace; the tabs re-read on it.
    turn: number;
    active: string | null;
    on_active: (id: string | null, repositories: string[] | null) => void;
    /// Called when a tab activates a workspace, so the rest of the window —
    /// the rail, the trail — re-reads instead of waiting for its next poll.
    on_switched: () => void;
    counts: Record<string, number>;
}

export function WorkspaceTabs({ turn, active, on_active, on_switched, counts }: Props) {
    const [workspaces, set_workspaces] = useState<Workspace[]>([]);
    const [repos, set_repos] = useState<Repository[]>([]);
    const [editing, set_editing] = useState<string | null>(null);
    const [drafting, set_drafting] = useState(false);
    const [name, set_name] = useState("");
    const [error, set_error] = useState<string | null>(null);

    // The App hands over a fresh on_active on every render, and answering it
    // makes the App render. Depending on it here made refresh a new function
    // each time, which re-ran the effect below, which answered again: measured
    // at 120 reads of /workspaces and /repos a second, and a webview main
    // thread at 70% with nothing on screen changing. The latest callback is
    // read through a ref instead, so refresh is one function for the life of
    // the tabs and the effect runs only when a turn says to.
    const answer = useRef(on_active);
    answer.current = on_active;

    const refresh = useCallback(async () => {
        const [listed, all] = await Promise.all([list_workspaces(), list_repos()]);
        set_workspaces(listed.workspaces);
        set_repos(all);

        const chosen = listed.workspaces.find((entry) => entry.id === listed.active) ?? null;
        answer.current(chosen?.id ?? null, chosen ? chosen.repository_ids : null);
    }, []);

    useEffect(() => {
        refresh().catch((cause) => set_error(cause instanceof Error ? cause.message : String(cause)));
    }, [refresh, turn]);

    const choose = useCallback(
        (id: string) => {
            set_editing(null);
            activate_workspace(id)
                .then(() => {
                    on_switched();
                    return refresh();
                })
                .catch((cause) => set_error(cause instanceof Error ? cause.message : String(cause)));
        },
        [refresh, on_switched],
    );

    const create = useCallback(() => {
        const trimmed = name.trim();
        if (!trimmed) {
            return;
        }

        create_workspace(trimmed, [])
            .then((created) => {
                set_name("");
                set_drafting(false);
                set_editing(created.id);
                return activate_workspace(created.id).then(() => refresh());
            })
            .catch((cause) => set_error(cause instanceof Error ? cause.message : String(cause)));
    }, [name, refresh]);

    const toggle_repo = useCallback(
        (workspace: Workspace, repository_id: string) => {
            const held = workspace.repository_ids.includes(repository_id)
                ? workspace.repository_ids.filter((id) => id !== repository_id)
                : [...workspace.repository_ids, repository_id];

            set_workspace_repos(workspace.id, held)
                .then(() => refresh())
                .catch((cause) => set_error(cause instanceof Error ? cause.message : String(cause)));
        },
        [refresh],
    );

    const current = workspaces.find((entry) => entry.id === editing) ?? null;

    return (
        <div className="relative flex items-center gap-1">
            {workspaces.map((workspace) => {
                const chosen = workspace.id === active;
                return (
                    <button
                        key={workspace.id}
                        onClick={() => (chosen ? set_editing(chosen && editing ? null : workspace.id) : choose(workspace.id))}
                        title={
                            chosen
                                ? "click again to choose its repositories"
                                : workspace.repository_ids.join(", ") || "no repositories yet"
                        }
                        className={`flex items-center gap-1.5 rounded px-2 py-[3px] text-[12px] ${
                            chosen ? "bg-lagoon text-linen" : "text-shell hover:text-linen"
                        }`}
                    >
                        <span>{workspace.name}</span>
                        {counts[workspace.id] ? (
                            <span className="font-mono text-[10px] tabular-nums text-shade">
                                {counts[workspace.id]}
                            </span>
                        ) : null}
                    </button>
                );
            })}

            {workspaces.length === 0 && !drafting ? (
                <span className="font-mono text-[11px] text-shade">
                    name a workspace to work in
                </span>
            ) : null}

            {drafting || workspaces.length === 0 ? (
                <input
                    // Focus follows the person who asked for the box. On a first
                    // run it appears on its own, and taking the cursor then would
                    // steal it from whatever they had come to do.
                    autoFocus={drafting}
                    className="w-28 rounded border border-reef bg-lagoon-deep px-1.5 py-[2px] text-[12px]"
                    placeholder="name"
                    value={name}
                    onChange={(event) => set_name(event.target.value)}
                    onBlur={() => set_drafting(false)}
                    onKeyDown={(event) => {
                        if (event.key === "Enter") {
                            create();
                        }
                        if (event.key === "Escape") {
                            set_drafting(false);
                        }
                    }}
                />
            ) : (
                <button
                    className="rounded px-1.5 py-[3px] font-mono text-[12px] text-shade hover:text-linen"
                    title="new workspace"
                    onClick={() => set_drafting(true)}
                >
                    +
                </button>
            )}

            {current ? (
                <div className="absolute left-0 top-full z-30 mt-1 w-60 rounded-lg border border-reef bg-lagoon-deep p-2 shadow-lg">
                    <div className="mb-1 font-mono text-[9px] uppercase tracking-[0.14em] text-shade">
                        Repositories in {current.name}
                    </div>
                    {repos.length === 0 ? (
                        <p className="font-mono text-[10px] text-shade">No repository is registered.</p>
                    ) : null}
                    {repos.map((repo) => {
                        const held = current.repository_ids.includes(repo.id);
                        return (
                            <button
                                key={repo.id}
                                onClick={() => toggle_repo(current, repo.id)}
                                className="flex w-full items-center gap-2 rounded px-1.5 py-1 text-left text-[12px] hover:bg-lagoon"
                            >
                                <span className={held ? "text-palm" : "text-shade"}>{held ? "✓" : "·"}</span>
                                <span className="truncate text-linen">{repo.name}</span>
                            </button>
                        );
                    })}
                    <div className="mt-1 flex justify-between border-t border-reef/70 pt-1">
                        <button
                            className="rounded px-1.5 py-0.5 font-mono text-[10px] text-coral hover:underline"
                            onClick={() => {
                                remove_workspace(current.id)
                                    .then(() => {
                                        set_editing(null);
                                        return refresh();
                                    })
                                    .catch((cause) =>
                                        set_error(cause instanceof Error ? cause.message : String(cause)),
                                    );
                            }}
                        >
                            delete
                        </button>
                        <button
                            className="rounded px-1.5 py-0.5 font-mono text-[10px] text-shell hover:text-linen"
                            onClick={() => set_editing(null)}
                        >
                            done
                        </button>
                    </div>
                </div>
            ) : null}

            {error ? <span className="font-mono text-[10px] text-coral">{error}</span> : null}
        </div>
    );
}
