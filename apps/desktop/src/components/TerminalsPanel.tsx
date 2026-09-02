import { use_poll } from "@/lib/poll";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AnimatePresence, motion } from "motion/react";

import { TerminalPane } from "@/components/TerminalPane";
import {
    is_tauri,
    list_sessions,
    list_windows,
    set_window,
    type PaneView,
    type SessionInfo,
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

    const name_of = useCallback(
        (id: string) => services.crew.find((agent) => agent.session_id === id)?.name ?? id,
        [services.crew],
    );

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

    if (services.sessions.length === 0) {
        return (
            <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-1 p-4 text-center">
                <p className="font-mono text-[11px] text-shell">No terminal is open.</p>
                <p className="font-mono text-[10px] text-shade">
                    Start an agent, or open a shell from the header.
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
                    label={(() => {
                        const held = services.crew.find((agent) => agent.session_id === session.id);
                        return held?.title ?? held?.name;
                    })()}
                    crowned={
                        services.crew.find((agent) => agent.session_id === session.id)?.role ===
                        "commander"
                    }
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
                    on_branch={(entry) => entry.cwd && services.open_shell_in(entry.cwd)}
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
                            {
                                label: "Close this terminal",
                                danger: true,
                                run: () => services.close_session(session.id),
                            },
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
