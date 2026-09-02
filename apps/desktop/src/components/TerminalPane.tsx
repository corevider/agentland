import { useEffect, useRef, useState } from "react";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";

import { ReadablePane } from "@/components/ReadablePane";
import { upgrade_soon } from "@/lib/gpu_queue";
import { use_poll } from "@/lib/poll";
import {
    format_bytes,
    format_elapsed,
    open_stream,
    resize_session,
    session_stats,
    write_input,
    type SessionInfo,
} from "@/lib/core";

/// Its own kind, so a task card dropped on the island and a terminal dragged
/// across the grid are never mistaken for one another.
export const PANE_DRAG = "text/agentland-pane";

const TAIL_LIMIT_BYTES = 48 * 1024;
const QUEUE_LIMIT_BYTES = TAIL_LIMIT_BYTES * 4;
const BACKGROUND_FLUSH_MS = 250;
const RESET_SEQUENCE = new TextEncoder().encode("\x1b[0m");

function format_tokens(tokens: number): string {
    if (tokens >= 1_000_000) {
        return `${(tokens / 1_000_000).toFixed(1)}M`;
    }
    if (tokens >= 1_000) {
        return `${(tokens / 1_000).toFixed(1)}k`;
    }
    return String(tokens);
}

export interface PaneMetrics {
    bytes: number;
    dropped_frames: number;
    collapsed_bytes: number;
    renderer: string;
}

interface Props {
    session: SessionInfo;
    label?: string;
    /// The project's commander. Marked because it is the one to talk to: it
    /// hands the work out and everybody else is working to what it decided.
    crowned?: boolean;
    on_close?: (id: string) => void;
    on_zoom?: (id: string) => void;
    zoomed?: boolean;
    on_branch?: (session: SessionInfo) => void;
    on_tear_out?: (session: SessionInfo) => void;
    stats_from?: SessionInfo | null;
    now_from?: number;
    readable?: boolean;
    on_readable?: (wanted: boolean) => void;
    on_menu?: (event: React.MouseEvent, where: "header" | "body") => void;
    /// Reordering: the header is the handle, the whole card is the target.
    on_pick_up?: (id: string) => void;
    on_drop_on?: (moved: string, target: string) => void;
    wanted?: boolean;
    focused: boolean;
    on_focus: (id: string) => void;
    on_metrics: (id: string, metrics: PaneMetrics) => void;
}

function concat(chunks: Uint8Array[], total: number): Uint8Array {
    const merged = new Uint8Array(total);
    let offset = 0;
    for (const chunk of chunks) {
        merged.set(chunk, offset);
        offset += chunk.byteLength;
    }
    return merged;
}

function collapse_to_tail(data: Uint8Array): Uint8Array {
    const tail = data.subarray(data.byteLength - TAIL_LIMIT_BYTES);
    const newline = tail.indexOf(10);
    const aligned = newline >= 0 ? tail.subarray(newline + 1) : tail;

    const result = new Uint8Array(RESET_SEQUENCE.byteLength + aligned.byteLength);
    result.set(RESET_SEQUENCE, 0);
    result.set(aligned, RESET_SEQUENCE.byteLength);
    return result;
}

export function TerminalPane({ session, crowned, focused, on_focus, on_metrics, label, on_close, on_zoom, zoomed, on_branch, on_tear_out, readable = false, on_readable, on_menu, stats_from, now_from, on_pick_up, on_drop_on, wanted = false }: Props) {
    const host_ref = useRef<HTMLDivElement>(null);
    const screen_ref = useRef<Terminal | null>(null);
    const gpu_ref = useRef<WebglAddon | null>(null);
    const readable_ref = useRef(readable);
    readable_ref.current = readable;
    const focused_ref = useRef(focused);
    const [renderer, set_renderer] = useState("canvas");
    const [stats, set_stats] = useState<SessionInfo | null>(null);
    const [now, set_now] = useState(() => Math.floor(Date.now() / 1000));

    focused_ref.current = focused;

    // A grid of panes shares one reading of the core and one clock rather than
    // each pane keeping its own: eight panes were eight requests and eight
    // re-renders a second, for one liveness dot and one elapsed time each.
    use_poll(() => {
        if (!stats_from) {
            session_stats(session.id).then(set_stats).catch(() => undefined);
        }
        if (now_from === undefined) {
            set_now(Math.floor(Date.now() / 1000));
        }
    }, 1000, stats_from === undefined || now_from === undefined);

    const shown_stats = stats_from ?? stats;
    const shown_now = now_from ?? now;

    useEffect(() => {
        const host = host_ref.current;
        if (!host) {
            return;
        }

        const metrics: PaneMetrics = {
            bytes: 0,
            dropped_frames: 0,
            collapsed_bytes: 0,
            renderer: "canvas",
        };

        const terminal = new Terminal({
            fontSize: 12,
            fontFamily: "'IBM Plex Mono', ui-monospace, monospace",
            scrollback: 500,
            theme: { background: "#0d1315", foreground: "#d6e2e6" },
        });

        screen_ref.current = terminal;

        let disposed = false;
        const fit = new FitAddon();
        terminal.loadAddon(fit);

        // Measured, not assumed: giving every pane a context beats reserving one
        // for whichever pane is being read. Eight panes on WebGL held 60 fps with
        // a 22 ms worst frame; handing seven of them to canvas dropped that to 49
        // with a 1357 ms stall, so the four-context probe does not bind here.
        //
        // What does bind is when the context arrives. Creating one costs about
        // 190 ms, so a pane opens on the canvas renderer and is upgraded a moment
        // later, one pane at a time.
        let webgl: WebglAddon | null = null;

        const take_gpu = () => {
            if (disposed || readable_ref.current) {
                return;
            }

            try {
                const addon = new WebglAddon();
                addon.onContextLoss(() => {
                    addon.dispose();
                    webgl = null;
                    gpu_ref.current = null;
                    metrics.renderer = "canvas (context lost)";
                    set_renderer("canvas");
                });
                terminal.loadAddon(addon);
                webgl = addon;
                gpu_ref.current = addon;
                metrics.renderer = "webgl";
                set_renderer("webgl");
            } catch (cause) {
                const reason = cause instanceof Error ? cause.message : String(cause);
                metrics.renderer = `canvas (${reason.slice(0, 60)})`;
                set_renderer("canvas");
            }
        };

        let cancel_upgrade = () => undefined as void;
        let queue: Uint8Array[] = [];
        let queued_bytes = 0;
        let writing = false;
        let frame_handle = 0;

        const flush = () => {
            if (writing || queued_bytes === 0) {
                return;
            }

            let payload = concat(queue, queued_bytes);
            queue = [];
            queued_bytes = 0;

            if (payload.byteLength > TAIL_LIMIT_BYTES) {
                metrics.collapsed_bytes += payload.byteLength - TAIL_LIMIT_BYTES;
                payload = collapse_to_tail(payload);
            }

            writing = true;
            terminal.write(payload, () => {
                writing = false;
            });
        };

        // Nothing to draw when nothing arrives: a pane that asks for a frame
        // every frame keeps the whole page pipeline awake, and eight idle panes
        // cost more than the terminals they are showing.
        const schedule = () => {
            if (disposed || frame_handle !== 0 || queued_bytes === 0) {
                return;
            }

            if (focused_ref.current) {
                frame_handle = requestAnimationFrame(() => {
                    frame_handle = 0;
                    flush();
                    schedule();
                });
                return;
            }

            frame_handle = window.setTimeout(() => {
                frame_handle = 0;
                flush();
                schedule();
            }, BACKGROUND_FLUSH_MS);
        };

        const flush_metrics = window.setInterval(() => {
            on_metrics(session.id, { ...metrics });
            metrics.bytes = 0;
        }, 1000);

        let socket: WebSocket | null = null;

        const connect = () =>
            open_stream(session.id).then((connection) => {
            if (disposed) {
                connection.close();
                return;
            }

            socket = connection;
            connection.onmessage = (event) => {
                if (typeof event.data === "string") {
                    const notice = JSON.parse(event.data) as { type: string; frames: number };
                    if (notice.type === "dropped") {
                        metrics.dropped_frames += notice.frames;
                    }
                    return;
                }

                const payload = new Uint8Array(event.data as ArrayBuffer);
                metrics.bytes += payload.byteLength;
                queue.push(payload);
                queued_bytes += payload.byteLength;

                schedule();

                if (queued_bytes > QUEUE_LIMIT_BYTES) {
                    const merged = concat(queue, queued_bytes);
                    metrics.collapsed_bytes += merged.byteLength - TAIL_LIMIT_BYTES;
                    const tail = collapse_to_tail(merged);
                    queue = [tail];
                    queued_bytes = tail.byteLength;
                }
            };
        });

        const input_disposable = terminal.onData((data) => {
            if (session.kind === "pty") {
                void write_input(session.id, data);
            }
        });

        const observer = new ResizeObserver(() => {
            fit.fit();
            if (session.kind === "pty") {
                void resize_session(session.id, terminal.cols, terminal.rows);
            }
        });

        // The heavy half of a pane — building the renderer, measuring the font,
        // laying out the grid — waits a step, so a page of eight appears at once
        // and fills in behind itself instead of blocking for 600 ms.
        const cancel_open = upgrade_soon(() => {
            if (disposed) {
                return;
            }

            terminal.open(host);
            fit.fit();
            observer.observe(host);
            void connect();
            cancel_upgrade = upgrade_soon(take_gpu);
        });

        return () => {
            disposed = true;
            cancelAnimationFrame(frame_handle);
            window.clearTimeout(frame_handle);
            cancel_open();
            cancel_upgrade();
            window.clearInterval(flush_metrics);
            observer.disconnect();
            webgl?.dispose();
            gpu_ref.current = null;
            input_disposable.dispose();
            socket?.close();
            screen_ref.current = null;
            terminal.dispose();
        };
    }, [session.id, session.kind, on_metrics]);

    useEffect(() => {
        if (!readable) {
            return;
        }

        // A pane being read as text is not being drawn, and a GL context with a
        // glyph atlas is the most expensive thing in it. The emulator keeps its
        // buffer either way — that is what the readable view reads.
        gpu_ref.current?.dispose();
        gpu_ref.current = null;
        set_renderer("canvas");

        return () => {
            const terminal = screen_ref.current;
            if (!terminal) {
                return;
            }

            try {
                const addon = new WebglAddon();
                terminal.loadAddon(addon);
                gpu_ref.current = addon;
                set_renderer("webgl");
            } catch {
                set_renderer("canvas");
            }
        };
    }, [readable]);

    const state = !shown_stats
        ? "opening"
        : !shown_stats.alive
          ? "exited"
          : now - shown_stats.last_output_at <= 2
            ? "working"
            : "waiting";

    const tint =
        state === "working"
            ? "bg-sun"
            : state === "exited"
              ? "bg-coral"
              : state === "waiting"
                ? "bg-palm"
                : "bg-shade";

    return (
        <div
            className={`flex min-h-0 flex-col overflow-hidden rounded-lg border bg-lagoon-deep ${
                wanted ? "border-sun" : focused ? "border-turquoise" : "border-reef"
            }`}
            onMouseDown={() => on_focus(session.id)}
            onDragOver={(event) => {
                if (on_drop_on && event.dataTransfer.types.includes(PANE_DRAG)) {
                    event.preventDefault();
                    event.dataTransfer.dropEffect = "move";
                }
            }}
            onDrop={(event) => {
                if (!on_drop_on) {
                    return;
                }

                // The dropped terminal names itself in the event; asking React
                // what was picked up would be asking a second source of truth.
                const moved = event.dataTransfer.getData(PANE_DRAG);
                if (moved) {
                    event.preventDefault();
                    on_drop_on(moved, session.id);
                }
            }}
            onContextMenu={(event) => on_menu?.(event, "body")}
        >
            <div
                className={`flex shrink-0 items-center gap-2 border-b border-reef/70 px-2 py-1 ${
                    on_pick_up ? "cursor-grab active:cursor-grabbing" : ""
                }`}
                draggable={Boolean(on_pick_up)}
                onDragStart={(event) => {
                    event.dataTransfer.setData(PANE_DRAG, session.id);
                    event.dataTransfer.effectAllowed = "move";
                    on_pick_up?.(session.id);
                }}
                onDragEnd={() => on_pick_up?.("")}
                onContextMenu={(event) => on_menu?.(event, "header")}
                title={on_pick_up ? "drag this bar to move the terminal" : undefined}
            >
                <span className={`size-[7px] shrink-0 rounded-full ${tint}`} title={state} />
                {crowned ? (
                    <span
                        className="shrink-0 text-[11px] leading-none"
                        title="the commander of this project — it hands the work out"
                        aria-label="commander"
                    >
                        ♔
                    </span>
                ) : null}
                <span
                    className={`truncate text-[12px] ${crowned ? "font-semibold text-sun" : "text-linen"}`}
                >
                    {label ?? session.id}
                </span>
                <span className="shrink-0 rounded bg-lagoon px-1 py-[1px] font-mono text-[9px] text-shade">
                    {session.command.split(/\s+/)[0]}
                </span>

                {shown_stats ? (
                    <span className="ml-auto flex shrink-0 items-center gap-2 font-mono text-[10px] tabular-nums text-shade">
                        {shown_stats.context_percent !== null ? (
                            <span className="text-turquoise">{shown_stats.context_percent}% ctx</span>
                        ) : shown_stats.context_tokens !== null ? (
                            <span className="text-turquoise" title="what the engine reports in context">
                                {format_tokens(shown_stats.context_tokens)} ctx
                            </span>
                        ) : null}
                        <span title={`${shown_stats.lines.toLocaleString()} lines since start`}>
                            {format_bytes(shown_stats.bytes)}
                        </span>
                    </span>
                ) : null}

                {on_branch && session.cwd ? (
                    <button
                        className="shrink-0 rounded px-1 font-mono text-[11px] text-shade hover:text-turquoise"
                        title="open another shell in the same worktree"
                        onClick={(event) => {
                            event.stopPropagation();
                            on_branch(session);
                        }}
                    >
                        +
                    </button>
                ) : null}

                {on_readable ? (
                    <button
                        className={`shrink-0 rounded px-1 font-mono text-[11px] ${
                            readable ? "text-turquoise" : "text-shade hover:text-turquoise"
                        }`}
                        title={readable ? "back to the terminal" : "read it as text, without the redrawing"}
                        onClick={(event) => {
                            event.stopPropagation();
                            on_readable(!readable);
                        }}
                    >
                        ¶
                    </button>
                ) : null}

                {on_tear_out ? (
                    <button
                        className="shrink-0 rounded px-1 font-mono text-[11px] text-shade hover:text-turquoise"
                        title="open this terminal in its own window"
                        onClick={(event) => {
                            event.stopPropagation();
                            on_tear_out(session);
                        }}
                    >
                        ⧉
                    </button>
                ) : null}

                {on_zoom ? (
                    <button
                        className="shrink-0 rounded px-1 font-mono text-[11px] text-shade hover:text-turquoise"
                        title={zoomed ? "back to the grid" : "fill the panel with this terminal"}
                        onClick={(event) => {
                            event.stopPropagation();
                            on_zoom(session.id);
                        }}
                    >
                        {zoomed ? "▤" : "⤢"}
                    </button>
                ) : null}

                {on_close ? (
                    <button
                        className="shrink-0 rounded px-1 font-mono text-[11px] text-shade hover:text-coral"
                        title="close this terminal"
                        onClick={(event) => {
                            event.stopPropagation();
                            on_close(session.id);
                        }}
                    >
                        ×
                    </button>
                ) : null}
            </div>

            <div className="relative min-h-0 flex-1">
                <div
                    ref={host_ref}
                    className={`absolute inset-0 overflow-hidden p-1 ${
                        readable ? "pointer-events-none opacity-0" : ""
                    }`}
                />
                {readable ? (
                    <div className="absolute inset-0 flex flex-col bg-lagoon-deep">
                        <ReadablePane screen={screen_ref} />
                    </div>
                ) : null}
            </div>

            <div className="flex shrink-0 items-center gap-2 border-t border-reef/70 px-2 py-[3px] font-mono text-[10px] text-shade">
                <span className={state === "working" ? "text-sun" : state === "exited" ? "text-coral" : "text-shell"}>
                    {state}
                </span>
                {shown_stats ? <span className="tabular-nums">{format_elapsed(shown_now - shown_stats.last_output_at)}</span> : null}
                <span className="ml-auto truncate">
                    {readable ? "readable · text only" : `${focused ? "live" : `${BACKGROUND_FLUSH_MS}ms`} · ${renderer}`}
                </span>
            </div>
        </div>
    );
}
