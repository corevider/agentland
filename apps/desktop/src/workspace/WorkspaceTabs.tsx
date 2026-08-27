import { useCallback, useEffect, useState } from "react";

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
    active: string | null;
    on_active: (id: string | null, repositories: string[] | null) => void;
    counts: Record<string, number>;
}

export function WorkspaceTabs({ active, on_active, counts }: Props) {
    const [workspaces, set_workspaces] = useState<Workspace[]>([]);
    const [repos, set_repos] = useState<Repository[]>([]);
    const [editing, set_editing] = useState<string | null>(null);
    const [drafting, set_drafting] = useState(false);
    const [name, set_name] = useState("");
    const [error, set_error] = useState<string | null>(null);

    const refresh = useCallback(async () => {
        const [listed, all] = await Promise.all([list_workspaces(), list_repos()]);
        set_workspaces(listed.workspaces);
        set_repos(all);

        const chosen = listed.workspaces.find((entry) => entry.id === listed.active) ?? null;
        on_active(chosen?.id ?? null, chosen ? chosen.repository_ids : null);
    }, [on_active]);

    useEffect(() => {
        refresh().catch((cause) => set_error(cause instanceof Error ? cause.message : String(cause)));
    }, [refresh]);

    const choose = useCallback(
        (id: string | null) => {
            set_editing(null);
            activate_workspace(id)
                .then(() => refresh())
                .catch((cause) => set_error(cause instanceof Error ? cause.message : String(cause)));
        },
        [refresh],
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
            <button
                onClick={() => choose(null)}
                title="every repository"
                className={`rounded px-2 py-[3px] text-[12px] ${
                    active === null ? "bg-lagoon text-linen" : "text-shell hover:text-linen"
                }`}
            >
                All
            </button>

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

            {drafting ? (
                <input
                    autoFocus
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
