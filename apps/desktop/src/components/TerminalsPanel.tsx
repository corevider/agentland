import { use_poll } from "@/lib/poll";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AnimatePresence, motion } from "motion/react";

import type { MenuItem } from "@/components/ContextMenu";
import { TerminalPane } from "@/components/TerminalPane";
import {
    create_worktree,
    is_tauri,
    list_engines,
    list_places,
    list_repos,
    list_sessions,
    list_windows,
    open_cli,
    set_window,
    shape_agent,
    spawn_default_shell,
    stop_agent,
    type Authority,
    type Engine,
    type PaneView,
    type Repository,
    type SessionInfo,
    type WorktreePlace,
} from "@/lib/core";
import {
    MOST_PANES,
    best_columns,
    edge,
    fits_readably,
    grid_shape,
    page_count,
    page_of,
    resize_tracks,
    to_template,
    tracks_for,
} from "@/lib/grid";
import { apply_order, move_onto, order_of, prune_order } from "@/lib/order";
import {
    folder_name,
    out_of_sight,
    place_label,
    places_in,
    settled_place,
    standing_of,
    type Place,
} from "@/lib/shells";

/// The place that is not a folder yet. No path is ever the empty string, so it
/// can stand for "cut a new worktree" without colliding with a real one.
const A_NEW_WORKTREE = "";

/// The program that is not an engine: a plain shell. No engine is called this,
/// so it can sit in the same list as the engines without being mistaken for one.
const A_TERMINAL = "terminal";
import { use_services } from "@/workspace/registry";

const SIZES_KEY = "agentland-pane-grid-2";
const ORDER_KEY = "agentland-pane-order";

function load_order(): string[] {
    try {
        const raw = localStorage.getItem(ORDER_KEY);
        const held = raw ? (JSON.parse(raw) as unknown) : null;
        return Array.isArray(held) ? held.filter((id): id is string => typeof id === "string") : [];
    } catch {
        // An arrangement that cannot be read is not worth an error.
        return [];
    }
}

interface Sizes {
    /// null means "fit the panel" — the arrangement follows the space there is.
    wanted_columns: number | null;
    columns: number[];
    rows: number[];
}

function load_sizes(): Sizes {
    try {
        const raw = localStorage.getItem(SIZES_KEY);
        if (raw) {
            const held = JSON.parse(raw) as Partial<Sizes>;
            return {
                wanted_columns: held.wanted_columns ?? null,
                columns: Array.isArray(held.columns) ? held.columns : [],
                rows: Array.isArray(held.rows) ? held.rows : [],
            };
        }
    } catch {
        // A stored layout that cannot be read is not worth an error.
    }

    return { wanted_columns: null, columns: [], rows: [] };
}

export function TerminalsPanel({ active }: { active: boolean }) {
    const services = use_services();
    const [zoomed, set_zoomed] = useState<string | null>(null);
    const [page, set_page] = useState(0);
    const [sizes, set_sizes] = useState<Sizes>(load_sizes);
    const [resizing, set_resizing] = useState(false);
    const [order, set_order] = useState<string[]>(load_order);
    const [carried, set_carried] = useState<string | null>(null);
    const frame = useRef<HTMLElement | null>(null);
    const observer = useRef<ResizeObserver | null>(null);
    const [space, set_space] = useState({ width: 0, height: 0 });
    const drag = useRef<{ axis: "column" | "row"; gap: number } | null>(null);
    const [views, set_views] = useState<Record<string, PaneView>>({});
    const [live, set_live] = useState<Record<string, SessionInfo>>({});
    const [now, set_now] = useState(() => Math.floor(Date.now() / 1000));

    use_poll(() => {
        list_windows().then(set_views).catch(() => undefined);
    }, 3000, active);

    // Worktrees are cut and removed by hand, so this is read slowly. It is the
    // one reading behind both the label under a pane and the form's places.
    use_poll(() => {
        void refresh_places().catch(() => undefined);
    }, 8000, active);

    // One reading a second for the whole grid: the panes want a liveness dot and
    // a byte count, and that is one request either way.
    use_poll(() => {
        set_now(Math.floor(Date.now() / 1000));
        list_sessions()
            .then((current) =>
                set_live(Object.fromEntries(current.map((entry) => [entry.id, entry]))),
            )
            .catch(() => undefined);
    }, 1000, active);

    const tear_out = useCallback((id: string, title: string) => {
        set_window(id, { holder: "window" })
            .then(() => (is_tauri() ? invoke("open_pane_window", { sessionId: id, title }) : undefined))
            .then(() => list_windows().then(set_views))
            .catch(() => undefined);
    }, []);

    const agent_of = useCallback(
        (id: string) => services.crew.find((agent) => agent.session_id === id),
        [services.crew],
    );

    const name_of = useCallback(
        (id: string) =>
            views[id]?.title || services.crew.find((agent) => agent.session_id === id)?.name || id,
        [services.crew, views],
    );

    // A shell opens where a person points: here, in another of the project's
    // worktrees, in its main checkout, or in a worktree made on the spot. The
    // menu is read from the core when it opens, so it lists what exists now.
    // A CLI wants four answers where a shell wants one, which is a form rather
    // than a fourth level of menu. The places are the ones the menu just read,
    // so the form never disagrees with the menu it was opened from.
    const [known, set_known] = useState<{ repos: Repository[]; trees: WorktreePlace[] }>({
        repos: [],
        trees: [],
    });
    const [engines, set_engines] = useState<Engine[]>([]);
    const [starting, set_starting] = useState<{
        repository_id: string;
        cwd: string;
        name: string;
        engine_id: string;
        authority: Authority;
        x: number;
        y: number;
    } | null>(null);
    const [starting_error, set_starting_error] = useState<string | null>(null);
    const [renaming, set_renaming] = useState<{ id: string; title: string; x: number; y: number } | null>(null);

    const refresh_places = useCallback(async () => {
        const [repos, trees] = await Promise.all([list_repos(), list_places()]);
        const mine = repos.filter(
            (repo) => !services.repositories || services.repositories.includes(repo.id),
        );
        const held = {
            repos: mine,
            trees: trees.filter((tree) => mine.some((repo) => repo.id === tree.repository_id)),
        };
        set_known(held);
        return held;
    }, [services.repositories]);

    const open_shell_menu = useCallback(
        async (event: React.MouseEvent, cwd: string | null) => {
            const at = {
                clientX: event.clientX,
                clientY: event.clientY,
                preventDefault: () => undefined,
                stopPropagation: () => undefined,
            } as unknown as React.MouseEvent;

            const { repos, trees } = await refresh_places();

            const going = services.going;
            const from =
                cwd ??
                (going?.repository_id
                    ? (trees.find((tree) => tree.repository_id === going.repository_id && tree.name === going.worktree)?.path ??
                      repos.find((repo) => repo.id === going.repository_id)?.primary_path ??
                      null)
                    : null);
            const here = standing_of(from, repos, trees);

            const items: MenuItem[] = [];

            // What is still running with nothing on screen to reach it by. An
            // agent whose pane was hidden keeps working, and this is the only
            // way back to it.
            const waiting = out_of_sight(
                Object.values(live),
                services.sessions,
                (id) => views[id]?.holder,
            ).filter((entry) => {
                const held = agent_of(entry.id);
                return (
                    !held || !services.repositories || services.repositories.includes(held.repository_id)
                );
            });

            if (waiting.length > 0) {
                items.push({ label: "still running, not on screen", disabled: true });
                for (const entry of waiting) {
                    const held = agent_of(entry.id);
                    items.push({
                        label: held ? held.title || held.name : entry.command.split(/\s+/)[0],
                        hint: place_label(entry.cwd, known.repos, known.trees) ?? undefined,
                        run: () => services.open_session(entry.id),
                    });
                }
            }

            if (from) {
                items.push({
                    label: `Here · ${here?.worktree ?? (here ? "main checkout" : folder_name(from))}`,
                    hint: folder_name(from),
                    run: () => services.open_shell_in(from),
                });
            }

            const project_items = (repo: (typeof repos)[number]): MenuItem[] => [
                {
                    label: `main checkout · ${repo.default_branch}`,
                    hint: repo.missing ? `gone from disk · ${repo.primary_path}` : folder_name(repo.primary_path),
                    disabled: Boolean(repo.missing),
                    run: () => services.open_shell_in(repo.primary_path),
                },
                ...trees
                    .filter((tree) => tree.repository_id === repo.id)
                    .map((tree) => {
                        const standing = services.crew
                            .filter((agent) => agent.repository_id === repo.id && agent.worktree === tree.name)
                            .map((agent) => agent.name);
                        return {
                            label: `${tree.name} · ${tree.branch}`,
                            hint: tree.missing ? "gone from disk" : standing.length > 0 ? standing.join(", ") : `:${tree.port}`,
                            disabled: tree.missing,
                            run: () => services.open_shell_in(tree.path),
                        };
                    }),
                {
                    label: "New worktree…",
                    hint: repo.missing ? "its checkout is gone" : "+",
                    disabled: Boolean(repo.missing),
                    run: () => {
                        set_starting_error(null);
                        set_starting({
                            repository_id: repo.id,
                            cwd: A_NEW_WORKTREE,
                            name: "",
                            engine_id: A_TERMINAL,
                            authority: "own",
                            x: at.clientX,
                            y: at.clientY,
                        });
                    },
                },
            ];

            if (repos.length === 1) {
                items.push(...project_items(repos[0]));
            } else {
                for (const repo of repos) {
                    items.push({ label: repo.name, items: project_items(repo) });
                }
            }

            if (repos.length > 0) {
                const standing = here ?? {
                    repository_id: repos[0].id,
                    worktree: null,
                    path: repos[0].primary_path,
                };
                items.push({
                    label: "Start a CLI…",
                    hint: "claude, codex, and the rest",
                    run: () => {
                        set_starting_error(null);
                        set_starting({
                            repository_id: standing.repository_id,
                            cwd: standing.path,
                            name: "",
                            engine_id: "",
                            authority: "own",
                            x: at.clientX,
                            y: at.clientY,
                        });
                    },
                });
            }

            if (items.length === 0) {
                items.push({ label: "No project in this workspace yet", disabled: true });
            }

            services.open_menu(at, "Another shell", items);
        },
        [agent_of, known, live, refresh_places, services, views],
    );

    // The engines are probed by running each one, so they are read when the
    // form opens rather than kept fresh against a poll nobody is watching.
    // Where this project's CLI could open: its folders that are actually there,
    // and a worktree it has not cut yet.
    const cli_places = useMemo((): Place[] => {
        if (!starting) {
            return [];
        }

        const held = places_in(known, starting.repository_id);
        const project = known.repos.find((repo) => repo.id === starting.repository_id);

        return project && !project.missing
            ? [...held, { path: A_NEW_WORKTREE, label: "new worktree…" }]
            : held;
    }, [known, starting]);

    // A select whose value is none of its options shows the first one and holds
    // the other, so the form read "ada-tree" while carrying the path of a
    // checkout that had been deleted — and start sent the deleted one. The
    // value is corrected to something actually offered instead.
    useEffect(() => {
        set_starting((held) => {
            if (!held || cli_places.length === 0) {
                return held;
            }
            const settled = settled_place(held.cwd, cli_places);
            return settled === held.cwd ? held : { ...held, cwd: settled };
        });
    }, [cli_places]);

    const cli_form_open = starting !== null;
    useEffect(() => {
        if (!cli_form_open) {
            return;
        }
        list_engines()
            .then(set_engines)
            .catch(() => undefined);
    }, [cli_form_open]);

    // The form opens before the engines are known, so the first installed one
    // becomes the choice as soon as there is one to choose.
    useEffect(() => {
        const first = engines.find((engine) => engine.installed);
        if (first) {
            set_starting((held) => (held && !held.engine_id ? { ...held, engine_id: first.id } : held));
        }
    }, [engines]);

    /// A pane's name is kept wherever it can outlive the pane.
    ///
    /// Panes are the core's own children, so a core restart gives every one of
    /// them a new id — a name filed under the old id would be lost, or worse,
    /// land on whichever pane took that id next. An agent survives the restart
    /// and comes back to a fresh pane, so its name is kept on the agent. A
    /// shell nobody was hired into does not come back at all, and its name has
    /// nothing to outlive.
    const rename_pane = useCallback(
        (title: string) => {
            if (!renaming) {
                return;
            }

            const held = agent_of(renaming.id);
            if (held) {
                shape_agent(held.id, { title })
                    .then(() => services.refresh_crew())
                    .catch(() => undefined);
            } else {
                set_window(renaming.id, { title })
                    .then(set_views)
                    .catch(() => undefined);
            }

            set_renaming(null);
        },
        [agent_of, renaming, services],
    );

    const start_cli = useCallback(() => {
        if (!starting || !starting.engine_id) {
            return;
        }
        const wanted = starting;
        set_starting_error(null);

        const place =
            wanted.cwd === A_NEW_WORKTREE
                ? create_worktree(wanted.repository_id, wanted.name.trim()).then((made) => made.path)
                : Promise.resolve(wanted.cwd);

        place
            .then((cwd) =>
                wanted.engine_id === A_TERMINAL
                    ? spawn_default_shell(cwd)
                    : open_cli({
                          engine_id: wanted.engine_id,
                          cwd,
                          authority: wanted.authority,
                          repository_id: wanted.repository_id,
                      }),
            )
            .then((created) => {
                set_starting(null);
                services.adopt_session(created);
            })
            .catch((cause) => set_starting_error(cause instanceof Error ? cause.message : String(cause)));
    }, [services, starting]);

    // With no explicit arrangement, the panel shows what it can show properly.
    const room = sizes.wanted_columns === null
        ? fits_readably(space.width, space.height)
        : MOST_PANES;
    // The core lists terminals in the order they were started; this panel keeps
    // the order the crew was arranged in.
    const arranged = useMemo(() => apply_order(services.sessions, order), [order, services.sessions]);

    useEffect(() => {
        const alive = order_of(services.sessions);
        set_order((held) => {
            const tidy = prune_order(held, alive);
            return tidy.length === held.length ? held : tidy;
        });
    }, [services.sessions]);

    useEffect(() => {
        try {
            localStorage.setItem(ORDER_KEY, JSON.stringify(order));
        } catch {
            // Storage can be refused; an arrangement is not worth an error.
        }
    }, [order]);

    const rearrange = useCallback(
        (moved: string, target: string) => {
            // The arrangement on screen is the one to move within: the stored
            // order may not mention a terminal opened a moment ago, and that one
            // has to be draggable too.
            set_order(move_onto(order_of(arranged), moved, target));
        },
        [arranged],
    );

    const pages = page_count(services.sessions.length, room);
    // Closing terminals can leave the panel on a page that no longer exists.
    const current_page = Math.min(page, pages - 1);

    const shown = useMemo(() => {
        if (zoomed) {
            return arranged.filter((entry) => entry.id === zoomed);
        }

        return page_of(arranged, current_page, room);
    }, [arranged, current_page, room, zoomed]);

    // The panel is measured rather than assumed: the same eight terminals want
    // four columns in a wide strip and one in a narrow column. The observer is
    // attached through the ref itself, because the panel renders its empty state
    // first and the grid only appears once a terminal is open.
    const watch_space = useCallback((node: HTMLElement | null) => {
        frame.current = node;
        observer.current?.disconnect();

        if (!node) {
            return;
        }

        const measure = () => {
            const box = node.getBoundingClientRect();
            set_space((held) =>
                Math.abs(held.width - box.width) < 8 && Math.abs(held.height - box.height) < 8
                    ? held
                    : { width: box.width, height: box.height },
            );
        };

        measure();
        observer.current = new ResizeObserver(measure);
        observer.current.observe(node);
    }, []);

    useEffect(() => () => observer.current?.disconnect(), []);

    const fitted_columns = best_columns(shown.length, space.width, space.height);
    const shape = grid_shape(shown.length, zoomed ? 1 : (sizes.wanted_columns ?? fitted_columns));
    const columns = tracks_for(shape.columns, sizes.columns);
    const rows = tracks_for(shape.rows, sizes.rows);

    useEffect(() => {
        try {
            localStorage.setItem(SIZES_KEY, JSON.stringify(sizes));
        } catch {
            // Storage can be full or refused; the sizes are not worth an error.
        }
    }, [sizes]);

    // The listeners are installed once and read the drag through a ref: rebuilding
    // them on every move is what made the workspace dividers let go after a step.
    const latest = useRef({ columns, rows });
    latest.current = { columns, rows };

    useEffect(() => {
        const move = (event: PointerEvent) => {
            const held = drag.current;
            const bounds = frame.current?.getBoundingClientRect();
            if (!held || !bounds) {
                return;
            }

            event.preventDefault();

            const tracks = held.axis === "column" ? latest.current.columns : latest.current.rows;
            const total = tracks.reduce((sum, value) => sum + value, 0);
            const before = tracks.slice(0, held.gap).reduce((sum, value) => sum + value, 0);
            const pair = tracks[held.gap] + tracks[held.gap + 1];

            const along =
                held.axis === "column"
                    ? (event.clientX - bounds.left) / bounds.width
                    : (event.clientY - bounds.top) / bounds.height;

            const share = (along * total - before) / pair;

            set_sizes((current) =>
                held.axis === "column"
                    ? { ...current, columns: resize_tracks(tracks, held.gap, share) }
                    : { ...current, rows: resize_tracks(tracks, held.gap, share) },
            );
        };

        const stop = () => {
            if (!drag.current) {
                return;
            }

            drag.current = null;
            document.body.style.cursor = "";
            document.body.style.userSelect = "";
            set_resizing(false);
        };

        window.addEventListener("pointermove", move, { passive: false });
        window.addEventListener("pointerup", stop);
        window.addEventListener("pointercancel", stop);
        window.addEventListener("blur", stop);

        return () => {
            window.removeEventListener("pointermove", move);
            window.removeEventListener("pointerup", stop);
            window.removeEventListener("pointercancel", stop);
            window.removeEventListener("blur", stop);
        };
    }, []);

    // Both the empty panel and the grid can open these, so both render them.
    // The form lived only in the grid's branch once, which made "New worktree…"
    // do nothing at all on an empty panel.
    const popovers = (
        <>
            {renaming ? (
                <div
                    className="fixed z-50 flex flex-col gap-1.5 rounded-md border border-reef bg-lagoon-deep p-2 shadow-[0_10px_24px_rgba(0,0,0,0.45)]"
                    style={{
                        left: Math.min(renaming.x, window.innerWidth - 280),
                        top: Math.min(renaming.y, window.innerHeight - 90),
                    }}
                    onPointerDown={(event) => event.stopPropagation()}
                >
                    <span className="font-mono text-[10px] uppercase tracking-[0.12em] text-shade">
                        rename this pane
                    </span>
                    <div className="flex items-center gap-1">
                        <input
                            autoFocus
                            className="w-48 rounded border border-reef bg-lagoon px-2 py-1 font-mono text-[11px] text-linen"
                            placeholder={name_of(renaming.id)}
                            value={renaming.title}
                            onChange={(event) => set_renaming({ ...renaming, title: event.target.value })}
                            onKeyDown={(event) => {
                                if (event.key === "Enter") {
                                    rename_pane(renaming.title.trim());
                                }
                                if (event.key === "Escape") {
                                    set_renaming(null);
                                }
                            }}
                        />
                        <button
                            className="rounded border border-turquoise px-2 py-1 font-mono text-[11px] text-turquoise"
                            onClick={() => rename_pane(renaming.title.trim())}
                        >
                            name it
                        </button>
                        <button
                            className="px-1 font-mono text-[11px] text-shade hover:text-linen"
                            title="back to whatever it would be called otherwise"
                            onClick={() => rename_pane("")}
                        >
                            ×
                        </button>
                    </div>
                </div>
            ) : null}

            {starting ? (
                <div
                    className="fixed z-50 flex w-[300px] flex-col gap-2 rounded-md border border-reef bg-lagoon-deep p-2.5 shadow-[0_10px_24px_rgba(0,0,0,0.45)]"
                    style={{ left: Math.min(starting.x, window.innerWidth - 316), top: Math.min(starting.y, window.innerHeight - 260) }}
                    onPointerDown={(event) => event.stopPropagation()}
                    onKeyDown={(event) => {
                        if (event.key === "Escape") {
                            set_starting(null);
                        }
                    }}
                >
                    <span className="font-mono text-[10px] uppercase tracking-[0.12em] text-shade">open a pane</span>

                    <label className="flex items-center gap-2">
                        <span className="w-16 shrink-0 font-mono text-[10px] text-shade">project</span>
                        <select
                            className="min-w-0 flex-1 rounded border border-reef bg-lagoon px-1.5 py-1 font-mono text-[11px] text-linen"
                            value={starting.repository_id}
                            onChange={(event) => {
                                const wanted = event.target.value;
                                const offered = places_in(known, wanted);
                                set_starting({
                                    ...starting,
                                    repository_id: wanted,
                                    cwd: offered[0]?.path ?? A_NEW_WORKTREE,
                                    name: "",
                                });
                            }}
                        >
                            {known.repos.map((repo) => (
                                <option key={repo.id} value={repo.id}>
                                    {repo.name}
                                </option>
                            ))}
                        </select>
                    </label>

                    <label className="flex items-center gap-2">
                        <span className="w-16 shrink-0 font-mono text-[10px] text-shade">place</span>
                        {cli_places.length === 0 ? (
                            <span className="min-w-0 flex-1 font-mono text-[11px] text-coral">
                                nowhere to open — its checkout is gone and it has no worktrees
                            </span>
                        ) : (
                            <select
                                className="min-w-0 flex-1 rounded border border-reef bg-lagoon px-1.5 py-1 font-mono text-[11px] text-linen"
                                value={starting.cwd}
                                onChange={(event) => set_starting({ ...starting, cwd: event.target.value })}
                            >
                                {cli_places.map((place) => (
                                    <option key={place.path} value={place.path}>
                                        {place.label}
                                    </option>
                                ))}
                            </select>
                        )}
                    </label>

                    {starting.cwd === A_NEW_WORKTREE ? (
                        <label className="flex items-center gap-2">
                            <span className="w-16 shrink-0 font-mono text-[10px] text-shade">named</span>
                            <input
                                autoFocus
                                className="min-w-0 flex-1 rounded border border-reef bg-lagoon px-2 py-1 font-mono text-[11px] text-linen"
                                placeholder="a name, e.g. spike"
                                value={starting.name}
                                onChange={(event) => set_starting({ ...starting, name: event.target.value })}
                                onKeyDown={(event) => {
                                    if (event.key === "Enter") {
                                        start_cli();
                                    }
                                }}
                            />
                        </label>
                    ) : null}

                    {known.repos.find((repo) => repo.id === starting.repository_id)?.missing ? (
                        <span className="font-mono text-[10px] text-shade">
                            its main checkout is gone from disk, so only its worktrees are offered
                        </span>
                    ) : null}

                    <label className="flex items-center gap-2">
                        <span className="w-16 shrink-0 font-mono text-[10px] text-shade">program</span>
                        <select
                            className="min-w-0 flex-1 rounded border border-reef bg-lagoon px-1.5 py-1 font-mono text-[11px] text-linen"
                            value={starting.engine_id}
                            onChange={(event) => set_starting({ ...starting, engine_id: event.target.value })}
                        >
                            <option value={A_TERMINAL}>terminal · your own shell</option>
                            {engines.length === 0 ? <option value="">reading what is installed…</option> : null}
                            {engines
                                .filter((engine) => engine.installed)
                                .map((engine) => (
                                    <option key={engine.id} value={engine.id}>
                                        {engine.name}
                                        {engine.version ? ` · ${engine.version}` : ""}
                                    </option>
                                ))}
                        </select>
                    </label>

                    {starting.engine_id === A_TERMINAL ? null : (
                        <div className="flex flex-col gap-1">
                            <span className="font-mono text-[10px] text-shade">authority</span>
                            {(
                                [
                                    ["own", "on its own", "your config, your connectors"],
                                    [
                                        "crew",
                                        "with the crew's tools",
                                        "Agentland's tools, this project's permits, the house rules",
                                    ],
                                ] as [Authority, string, string][]
                            ).map(([value, label, hint]) => (
                                <label key={value} className="flex cursor-pointer items-start gap-1.5">
                                    <input
                                        type="radio"
                                        className="mt-[3px] accent-turquoise"
                                        checked={starting.authority === value}
                                        onChange={() => set_starting({ ...starting, authority: value })}
                                    />
                                    <span className="min-w-0">
                                        <span className="font-mono text-[11px] text-linen">{label}</span>
                                        <span className="block font-mono text-[10px] leading-tight text-shade">{hint}</span>
                                    </span>
                                </label>
                            ))}
                        </div>
                    )}

                    <div className="flex items-center justify-end gap-1">
                        <button
                            className="px-1.5 py-1 font-mono text-[11px] text-shade hover:text-linen"
                            onClick={() => set_starting(null)}
                        >
                            cancel
                        </button>
                        <button
                            className="rounded border border-turquoise px-2 py-1 font-mono text-[11px] text-turquoise disabled:opacity-40"
                            disabled={
                                !starting.engine_id ||
                                cli_places.length === 0 ||
                                (starting.cwd === A_NEW_WORKTREE && !starting.name.trim())
                            }
                            onClick={start_cli}
                        >
                            start
                        </button>
                    </div>

                    {engines.length > 0 && !engines.some((engine) => engine.installed) ? (
                        <span className="font-mono text-[10px] text-shade">
                            No engine is on PATH — a terminal is all there is to open.
                        </span>
                    ) : null}
                    {starting_error ? (
                        <span className="font-mono text-[10px] text-coral">{starting_error}</span>
                    ) : null}
                </div>
            ) : null}
        </>
    );

    // The toolbar that carries "+ shell" only renders once a terminal is open,
    // so the empty panel has to carry its own way out. It pointed at a control
    // in the header that does not exist, which left the one state where a
    // person most wants a terminal as the one state with no way to open one.
    if (services.sessions.length === 0) {
        return (
            <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 p-4 text-center">
                {popovers}
                <p className="font-mono text-[11px] text-shell">No terminal is open.</p>
                <button
                    className="rounded border border-reef px-2 py-1 font-mono text-[11px] text-shell hover:border-turquoise hover:text-linen"
                    onClick={(event) => void open_shell_menu(event, null)}
                >
                    + shell
                </button>
                <p className="font-mono text-[10px] text-shade">
                    in a project's checkout, one of its worktrees, or a new one
                </p>
            </div>
        );
    }

    const start_drag = (axis: "column" | "row", gap: number) => (event: React.PointerEvent) => {
        event.preventDefault();
        event.currentTarget.setPointerCapture?.(event.pointerId);
        drag.current = { axis, gap };
        set_resizing(true);
        document.body.style.cursor = axis === "column" ? "col-resize" : "row-resize";
        document.body.style.userSelect = "none";
    };

    return (
        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
            {popovers}
            <div
                data-chrome
                className="flex shrink-0 items-center gap-2 border-b border-reef/60 px-2 py-1 font-mono text-[10px] text-shade"
            >
                <span className="whitespace-nowrap">
                    {services.sessions.length} open
                    {pages > 1 ? ` · ${shown.length} shown` : ""}
                </span>

                {pages > 1 && !zoomed ? (
                    <span className="flex items-center gap-1">
                        <button
                            className="rounded px-1 hover:text-linen disabled:opacity-40"
                            disabled={current_page === 0}
                            onClick={() => set_page(Math.max(0, current_page - 1))}
                        >
                            ‹
                        </button>
                        <span className="tabular-nums text-shell">
                            {current_page + 1}/{pages}
                        </span>
                        <button
                            className="rounded px-1 hover:text-linen disabled:opacity-40"
                            disabled={current_page >= pages - 1}
                            onClick={() => set_page(Math.min(pages - 1, current_page + 1))}
                        >
                            ›
                        </button>
                    </span>
                ) : null}

                <span className="ml-auto flex items-center gap-1">
                    <button
                        className="mr-2 rounded border border-reef px-1.5 hover:border-turquoise hover:text-linen"
                        title="open a shell — in a worktree, the main checkout, or a new worktree"
                        onClick={(event) => void open_shell_menu(event, null)}
                    >
                        + shell
                    </button>
                    <span className="text-shade">columns</span>
                    <button
                        className={`rounded px-1.5 ${
                            sizes.wanted_columns === null ? "text-turquoise" : "hover:text-linen"
                        }`}
                        title={`fit the panel — ${fitted_columns} across right now`}
                        onClick={() =>
                            set_sizes((current) => ({ ...current, wanted_columns: null, columns: [], rows: [] }))
                        }
                    >
                        fit
                    </button>
                    {[1, 2, 3, 4].map((count) => (
                        <button
                            key={count}
                            className={`rounded px-1.5 tabular-nums ${
                                sizes.wanted_columns === count ? "text-turquoise" : "hover:text-linen"
                            }`}
                            title={`${count} across`}
                            onClick={() =>
                                set_sizes((current) => ({ ...current, wanted_columns: count, columns: [], rows: [] }))
                            }
                        >
                            {count}
                        </button>
                    ))}
                    <button
                        className="rounded px-1.5 hover:text-linen"
                        title="even them out again"
                        onClick={() => set_sizes((current) => ({ ...current, columns: [], rows: [] }))}
                    >
                        even
                    </button>
                </span>
            </div>

            <main
                ref={watch_space}
                className="relative grid min-h-0 min-w-0 flex-1 gap-1.5 p-1.5"
                style={{
                    gridTemplateColumns: to_template(columns),
                    gridTemplateRows: to_template(rows),
                }}
            >
            <AnimatePresence initial={false}>
            {shown.map((session) =>
                views[session.id]?.holder === "window" ? (
                    <motion.article
                        key={session.id}
                        layout
                        initial={{ opacity: 0, scale: 0.98 }}
                        animate={{ opacity: 1, scale: 1 }}
                        exit={{ opacity: 0, scale: 0.98 }}
                        transition={{ duration: 0.14, ease: [0.2, 0, 0, 1] }}
                        className="flex min-h-0 flex-col items-center justify-center gap-1 rounded-lg border border-dashed border-reef bg-lagoon-deep p-3 text-center"
                    >
                        <span className="text-[12px] text-shell">
                            {services.crew.find((agent) => agent.session_id === session.id)?.name ??
                                session.id}
                        </span>
                        <span className="font-mono text-[10px] text-shade">
                            open in its own window
                        </span>
                        <button
                            className="mt-1 rounded border border-reef px-2 py-0.5 font-mono text-[10px] text-shell hover:border-foam"
                            onClick={() => {
                                set_window(session.id, { holder: "grid" })
                                    .then(() =>
                                        is_tauri()
                                            ? invoke("close_pane_window", { sessionId: session.id })
                                            : undefined,
                                    )
                                    .then(() => list_windows().then(set_views))
                                    .catch(() => undefined);
                            }}
                        >
                            bring it back
                        </button>
                    </motion.article>
                ) : (
                <TerminalPane
                    key={session.id}
                    session={session}
                    label={
                        views[session.id]?.title ||
                        agent_of(session.id)?.title ||
                        agent_of(session.id)?.name
                    }
                    place={place_label(session.cwd, known.repos, known.trees)}
                    crew_name={agent_of(session.id)?.name}
                    crew_role={agent_of(session.id)?.role}
                    crowned={(() => {
                        const role = services.crew.find(
                            (agent) => agent.session_id === session.id,
                        )?.role;
                        return role === "commander" || role === "chief";
                    })()}
                    kept={services.crew.some((agent) => agent.session_id === session.id)}
                    focused={
                        active &&
                        (services.focused_id
                            ? services.focused_id === session.id
                            : session.id === services.sessions[0]?.id)
                    }
                    stats_from={live[session.id] ?? session}
                    now_from={now}
                    on_pick_up={(id) => set_carried(id || null)}
                    on_drop_on={(moved, target) => {
                        rearrange(moved, target);
                        set_carried(null);
                    }}
                    wanted={Boolean(carried) && carried !== session.id}
                    on_focus={services.focus_pane}
                    on_close={services.close_session}
                    on_zoom={(id) => set_zoomed((held) => (held === id ? null : id))}
                    zoomed={zoomed === session.id}
                    on_add={(entry, event) => void open_shell_menu(event, entry.cwd)}
                    readable={views[session.id]?.readable ?? false}
                    on_readable={(wanted) => {
                        set_window(session.id, { readable: wanted })
                            .then(set_views)
                            .catch(() => undefined);
                    }}
                    on_menu={(event, where) => {
                        const readable = views[session.id]?.readable ?? false;
                        services.open_menu(event, name_of(session.id), [
                            {
                                label: readable ? "Back to the terminal" : "Read it as text",
                                hint: "¶",
                                run: () => {
                                    set_window(session.id, { readable: !readable })
                                        .then(set_views)
                                        .catch(() => undefined);
                                },
                            },
                            {
                                label: zoomed === session.id ? "Back to the grid" : "Fill the panel",
                                hint: "⤢",
                                run: () => set_zoomed((held) => (held === session.id ? null : session.id)),
                            },
                            {
                                label: "Open in its own window",
                                hint: "⧉",
                                run: () => tear_out(session.id, name_of(session.id)),
                            },
                            {
                                label: "Rename this pane…",
                                hint: views[session.id]?.title ? "✎" : undefined,
                                run: () =>
                                    set_renaming({
                                        id: session.id,
                                        title: views[session.id]?.title ?? "",
                                        x: event.clientX,
                                        y: event.clientY,
                                    }),
                            },
                            {
                                label: "Another shell in this worktree",
                                hint: "+",
                                disabled: !session.cwd,
                                run: () => {
                                    if (session.cwd) {
                                        services.open_shell_in(session.cwd);
                                    }
                                },
                            },
                            ...(where === "body"
                                ? [
                                      {
                                          label: "Copy what is on screen",
                                          run: async () => {
                                              const text = window.getSelection()?.toString();
                                              if (text) {
                                                  await navigator.clipboard.writeText(text);
                                              }
                                          },
                                          disabled: !window.getSelection()?.toString(),
                                      },
                                  ]
                                : []),
                            ...(() => {
                                const held = services.crew.find((agent) => agent.session_id === session.id);
                                return held
                                    ? [
                                          {
                                              label: "Hide this pane",
                                              hint: "it keeps running",
                                              run: () => services.close_session(session.id),
                                          },
                                          {
                                              label: `Stop ${held.name}`,
                                              hint: held.role === "commander" || held.role === "chief" ? "its context is lost" : undefined,
                                              danger: true,
                                              run: () => stop_agent(held.id).then(() => services.close_session(session.id)),
                                          },
                                      ]
                                    : [
                                          {
                                              label: "Close this terminal",
                                              danger: true,
                                              run: () => services.close_session(session.id),
                                          },
                                      ];
                            })(),
                        ]);
                    }}
                    on_tear_out={(entry) =>
                        tear_out(
                            entry.id,
                            services.crew.find((agent) => agent.session_id === entry.id)?.name ??
                                entry.id,
                        )
                    }
                    on_metrics={services.on_metrics}
                />
                ),
            )}
            </AnimatePresence>

            {/* One handle per gap: dragging it sizes the two panes either side of
                it, which is how a pane gets bigger without the others moving. The
                layer matches the grid's own box so a divider sits on its gap. */}
            <div className="pointer-events-none absolute inset-1.5">
            {!zoomed
                ? columns.slice(0, -1).map((_, gap) => (
                      <div
                          key={`column-${gap}`}
                          className="group pointer-events-auto absolute inset-y-0 z-20 w-3 -translate-x-1/2 cursor-col-resize"
                          style={{ left: `calc(${edge(columns, gap)} * 100%)` }}
                          title="drag to size these two"
                          onPointerDown={start_drag("column", gap)}
                      >
                          <div
                              className={`mx-auto h-full w-[3px] rounded transition-colors ${
                                  resizing ? "bg-turquoise" : "bg-transparent group-hover:bg-turquoise/70"
                              }`}
                          />
                      </div>
                  ))
                : null}

            {!zoomed
                ? rows.slice(0, -1).map((_, gap) => (
                      <div
                          key={`row-${gap}`}
                          className="group pointer-events-auto absolute inset-x-0 z-20 h-3 -translate-y-1/2 cursor-row-resize"
                          style={{ top: `calc(${edge(rows, gap)} * 100%)` }}
                          title="drag to size these two"
                          onPointerDown={start_drag("row", gap)}
                      >
                          <div
                              className={`my-auto h-[3px] w-full rounded transition-colors ${
                                  resizing ? "bg-turquoise" : "bg-transparent group-hover:bg-turquoise/70"
                              }`}
                          />
                      </div>
                  ))
                : null}

            </div>

            {resizing ? (
                // A terminal canvas swallows pointer moves, so the drag would stick
                // the moment the cursor crossed one.
                <div className="fixed inset-0 z-30" />
            ) : null}
        </main>
        </div>
    );
}
