import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { Waiting } from "@/components/Spinner";

import {
    activate_workspace,
    list_agents,
    list_repos,
    list_workspaces,
    list_worktrees,
    type WorktreeStatus,
} from "@/lib/core";
import {
    home_from,
    needs_switch,
    places_from,
    search_places,
    trail,
    type Place,
    type World,
} from "@/lib/places";
import { use_poll } from "@/lib/poll";

const KIND_WORD: Record<Place["kind"], string> = {
    workspace: "workspace",
    project: "project",
    worktree: "worktree",
    agent: "agent",
};

const KIND_TINT: Record<Place["kind"], string> = {
    workspace: "text-sun",
    project: "text-turquoise",
    worktree: "text-palm",
    agent: "text-shell",
};

interface Props {
    open: boolean;
    on_close: () => void;
    /// Where to go once a place is chosen: the workspace is switched here first
    /// if the place lives in another one.
    on_go: (place: Place) => void;
}

async function read_world(): Promise<World> {
    const [workspaces, repositories, agents] = await Promise.all([
        list_workspaces(),
        list_repos(),
        list_agents(),
    ]);

    const trees = await Promise.all(
        repositories.map((repository) => list_worktrees(repository.id).catch(() => [] as WorktreeStatus[])),
    );

    return {
        workspaces: workspaces.workspaces,
        active_workspace: workspaces.active,
        repositories,
        worktrees: trees.flat(),
        agents,
    };
}

/// One box that reaches everywhere.
///
/// Workspaces hold different folders, a project's folder is not its worktrees',
/// and an agent sits in one of those — four kinds of place a person has to move
/// between all day. Typing a few letters finds any of them, and choosing one
/// switches the workspace on the way if it has to.
export function Jumper({ open, on_close, on_go }: Props) {
    const [world, set_world] = useState<World | null>(null);
    const [query, set_query] = useState("");
    const [cursor, set_cursor] = useState(0);
    const box = useRef<HTMLInputElement>(null);

    useEffect(() => {
        if (!open) {
            return;
        }

        set_query("");
        set_cursor(0);
        read_world().then(set_world).catch(() => undefined);
        box.current?.focus();
    }, [open]);

    const found = useMemo(
        () =>
            world
                ? search_places(
                      places_from(world, home_from(world.repositories.map((repo) => repo.primary_path))),
                      query,
                  )
                : [],
        [world, query],
    );

    const go = useCallback(
        (place: Place) => {
            // A place whose project belongs to no workspace is one this cannot
            // switch to, and asking to stand nowhere is refused now — so it is
            // not asked. The jump still happens; only the switch is skipped.
            const switching =
                world && place.workspace_id && needs_switch(place, world.active_workspace)
                    ? activate_workspace(place.workspace_id)
                    : Promise.resolve(null);

            switching
                .catch(() => undefined)
                .then(() => {
                    on_go(place);
                    on_close();
                });
        },
        [world, on_go, on_close],
    );

    if (!open) {
        return null;
    }

    return (
        <div
            className="fixed inset-0 z-[60] flex items-start justify-center bg-lagoon-deep/60 pt-24"
            onMouseDown={on_close}
        >
            <div
                className="w-[34rem] overflow-hidden rounded-lg border border-foam bg-lagoon shadow-xl"
                onMouseDown={(event) => event.stopPropagation()}
            >
                <input
                    ref={box}
                    autoFocus
                    className="w-full border-b border-reef bg-lagoon-deep px-3 py-2 text-[13px] text-linen outline-none"
                    placeholder="go to a workspace, project, worktree or agent…"
                    value={query}
                    onChange={(event) => {
                        set_query(event.target.value);
                        set_cursor(0);
                    }}
                    onKeyDown={(event) => {
                        if (event.key === "Escape") {
                            on_close();
                        } else if (event.key === "ArrowDown") {
                            event.preventDefault();
                            set_cursor((held) => Math.min(held + 1, found.length - 1));
                        } else if (event.key === "ArrowUp") {
                            event.preventDefault();
                            set_cursor((held) => Math.max(held - 1, 0));
                        } else if (event.key === "Enter" && found[cursor]) {
                            go(found[cursor]);
                        }
                    }}
                />

                <div className="max-h-[24rem] overflow-y-auto py-1">
                    {found.length === 0 ? (
                        <p className="px-3 py-2 font-mono text-[10px] text-shade">
                            {world ? "nothing by that name" : <Waiting says="reading the workspaces…" />}
                        </p>
                    ) : null}

                    {found.map((place, index) => (
                        <button
                            key={place.id}
                            className={`block w-full px-3 py-1.5 text-left ${
                                index === cursor ? "bg-shallow" : "hover:bg-shallow/60"
                            }`}
                            onMouseEnter={() => set_cursor(index)}
                            onClick={() => go(place)}
                        >
                            <div className="flex items-baseline gap-2">
                                <span className="text-[12px] text-linen">{place.name}</span>
                                <span className={`font-mono text-[9px] ${KIND_TINT[place.kind]}`}>
                                    {KIND_WORD[place.kind]}
                                </span>
                                {world && needs_switch(place, world.active_workspace) ? (
                                    <span className="ml-auto font-mono text-[9px] text-sun">
                                        switches workspace
                                    </span>
                                ) : null}
                            </div>
                            <div className="truncate font-mono text-[9px] text-shade">{place.detail}</div>
                        </button>
                    ))}
                </div>
            </div>
        </div>
    );
}

/// Where the person is standing, spelled out: workspace, project, and the
/// worktree with its branch — the three that are easy to confuse when a project
/// folder and the folder an agent works in are different places.
export function PlaceTrail({
    repository_id,
    worktree,
    turn,
    on_open,
}: {
    repository_id: string | null;
    worktree: string | null;
    /// Changes the moment a workspace is activated anywhere in the window, so
    /// the trail says what is true now rather than at its last poll.
    turn: number;
    on_open: () => void;
}) {
    const [world, set_world] = useState<World | null>(null);

    const read = useCallback(() => {
        read_world().then(set_world).catch(() => undefined);
    }, []);

    useEffect(read, [read, turn]);
    use_poll(read, 5000);

    const crumbs = world
        ? trail(world, repository_id, worktree, home_from(world.repositories.map((repo) => repo.primary_path)))
        : [];

    return (
        <button
            className="flex shrink-0 items-baseline gap-1 rounded border border-transparent px-1.5 py-[3px] font-mono text-[10px] text-shell hover:border-reef hover:text-linen"
            title="go somewhere else — ctrl+k"
            onClick={on_open}
        >
            {crumbs.map((crumb, index) => (
                <span key={`${crumb}-${index}`} className="flex items-baseline gap-1">
                    {index > 0 ? <span className="text-shade">›</span> : null}
                    <span className={`max-w-[14rem] truncate ${index === crumbs.length - 1 ? "text-linen" : ""}`}>
                        {crumb}
                    </span>
                </span>
            ))}
            <span className="ml-1 shrink-0 text-shade">⌃K</span>
        </button>
    );
}
