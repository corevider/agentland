import { use_poll } from "@/lib/poll";
import { useCallback, useEffect, useRef, useState } from "react";

import {
    activate_workspace,
    create_workspace,
    list_repos,
    list_workspaces,
    remove_workspace,
    set_workspace_repos,
    suggest_a_chief,
    type Engine,
    type Repository,
    type Workspace,
} from "@/lib/core";
import { Picker } from "@/components/Picker";
import { Press } from "@/components/Press";
import { hireable_engines } from "@/lib/hiring_rules";

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
    const [chief, set_chief] = useState("");
    const [suggested, set_suggested] = useState("");
    const [engine_id, set_engine] = useState("");
    const [engines, set_engines] = useState<Engine[]>([]);
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

    use_poll(() => { refresh().catch(() => undefined); }, 10000);

    const choose = useCallback(
        (id: string) => {
            set_editing(null);
            return activate_workspace(id)
                .then(() => {
                    on_switched();
                    return refresh();
                })
                .catch((cause) => set_error(cause instanceof Error ? cause.message : String(cause)));
        },
        [refresh, on_switched],
    );

    // A workspace is made on Enter, in either box, and Enter held a moment too
    // long made two. One at a time.
    const making = useRef(false);
    const create = useCallback(() => {
        const trimmed = name.trim();
        if (!trimmed || making.current) {
            return;
        }

        making.current = true;

        // An empty field means the name it offered. The suggestion is not
        // written into the field, so a person who types nothing is agreeing to
        // what they can already read rather than to something invisible.
        create_workspace(trimmed, [], chief.trim() || suggested, engine_id)
            .then((created) => {
                set_name("");
                set_chief("");
                set_engine("");
                set_drafting(false);
                set_editing(created.id);
                return activate_workspace(created.id).then(() => refresh());
            })
            .catch((cause) => set_error(cause instanceof Error ? cause.message : String(cause)))
            .finally(() => {
                making.current = false;
            });
    }, [chief, engine_id, name, suggested, refresh]);

    // What this workspace's chief would be called, asked as the name is typed.
    // The core picks it: the list of names and the crew already answering to
    // some of them are both its business, not the panel's.
    useEffect(() => {
        const wanted = name.trim();
        if (!wanted) {
            set_suggested("");
            return;
        }

        const handle = window.setTimeout(() => {
            suggest_a_chief(wanted)
                .then((held) => set_suggested(held.chief))
                .catch(() => undefined);
        }, 250);

        return () => window.clearTimeout(handle);
    }, [name]);

    useEffect(() => {
        hireable_engines()
            .then(set_engines)
            .catch(() => undefined);
    }, []);

    const toggle_repo = useCallback(
        (workspace: Workspace, repository_id: string) => {
            const held = workspace.repository_ids.includes(repository_id)
                ? workspace.repository_ids.filter((id) => id !== repository_id)
                : [...workspace.repository_ids, repository_id];

            return set_workspace_repos(workspace.id, held)
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
                    <Press
                        key={workspace.id}
                        on_press={() => (chosen ? set_editing(chosen && editing ? null : workspace.id) : choose(workspace.id))}
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
                    </Press>
                );
            })}

            {workspaces.length === 0 && !drafting ? (
                <span className="font-mono text-[11px] text-shade">
                    name a workspace to work in
                </span>
            ) : null}

            {drafting || workspaces.length === 0 ? (
                <div
                    className="flex items-center gap-1"
                    // Closing on blur belongs to the fields together, not to
                    // any one: moving from the workspace's name to its chief's
                    // used to shut the form on the way. The engine list hangs
                    // from the window, so focus landing in it is still inside.
                    onBlur={(event) => {
                        const next = event.relatedTarget as Element | null;
                        if (!event.currentTarget.contains(next) && !next?.closest("[data-picker-popup]")) {
                            set_drafting(false);
                        }
                    }}
                >
                    <input
                        // Focus follows the person who asked for the box. On a
                        // first run it appears on its own, and taking the cursor
                        // then would steal it from whatever they had come to do.
                        autoFocus={drafting}
                        className="w-28 rounded border border-reef bg-lagoon-deep px-1.5 py-[2px] text-[12px]"
                        placeholder="name"
                        value={name}
                        onChange={(event) => set_name(event.target.value)}
                        onKeyDown={(event) => {
                            if (event.key === "Enter") {
                                create();
                            }
                            if (event.key === "Escape") {
                                set_drafting(false);
                            }
                        }}
                    />
                    <input
                        className="w-24 rounded border border-reef bg-lagoon-deep px-1.5 py-[2px] text-[12px]"
                        placeholder={suggested ? `chief · ${suggested}` : "chief"}
                        title="who commands this workspace — leave it be to take the name offered"
                        value={chief}
                        onChange={(event) => set_chief(event.target.value)}
                        onKeyDown={(event) => {
                            if (event.key === "Enter") {
                                create();
                            }
                            if (event.key === "Escape") {
                                set_drafting(false);
                            }
                        }}
                    />
                    <Picker
                        className="w-32 rounded border border-reef bg-lagoon-deep px-1.5 py-[2px] text-[12px]"
                        title="the engine this workspace's chief runs on"
                        value={engine_id}
                        choices={[
                            { value: "", label: "engine · whichever takes the crew's tools" },
                            ...engines.map((engine) => ({ value: engine.id, label: engine.name })),
                        ]}
                        on_pick={set_engine}
                    />
                </div>
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
                            <Press
                                key={repo.id}
                                on_press={() => toggle_repo(current, repo.id)}
                                className="flex w-full items-center gap-2 rounded px-1.5 py-1 text-left text-[12px] hover:bg-lagoon"
                            >
                                <span className={held ? "text-palm" : "text-shade"}>{held ? "✓" : "·"}</span>
                                <span className="truncate text-linen">{repo.name}</span>
                            </Press>
                        );
                    })}
                    <div className="mt-1 flex justify-between border-t border-reef/70 pt-1">
                        <Press
                            className="rounded px-1.5 py-0.5 font-mono text-[10px] text-coral hover:underline"
                            busy_says="deleting…"
                            on_press={() =>
                                remove_workspace(current.id)
                                    .then(() => {
                                        set_editing(null);
                                        return refresh();
                                    })
                                    .catch((cause) =>
                                        set_error(cause instanceof Error ? cause.message : String(cause)),
                                    )
                            }
                        >
                            delete
                        </Press>
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
