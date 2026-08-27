import { useEffect, useRef, useState } from "react";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";

import {
    format_bytes,
    format_elapsed,
    open_stream,
    resize_session,
    session_stats,
    write_input,
    type SessionInfo,
} from "@/lib/core";

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
    on_close?: (id: string) => void;
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

export function TerminalPane({ session, focused, on_focus, on_metrics, label, on_close}: Props) {
    const host_ref = useRef<HTMLDivElement>(null);
    const focused_ref = useRef(focused);
    const [renderer, set_renderer] = useState("canvas");
    const [stats, set_stats] = useState<SessionInfo | null>(null);
    const [now, set_now] = useState(() => Math.floor(Date.now() / 1000));

    focused_ref.current = focused;

    useEffect(() => {
        let cancelled = false;

        const poll = () => {
            session_stats(session.id)
                .then((value) => {
                    if (!cancelled) {
                        set_stats(value);
                    }
                })
                .catch(() => undefined);
            set_now(Math.floor(Date.now() / 1000));
        };

        poll();
        const handle = window.setInterval(poll, 1000);
        return () => {
            cancelled = true;
            window.clearInterval(handle);
        };
    }, [session.id]);

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

        const fit = new FitAddon();
        terminal.loadAddon(fit);
        terminal.open(host);

        try {
            const webgl = new WebglAddon();
            webgl.onContextLoss(() => webgl.dispose());
            terminal.loadAddon(webgl);
            metrics.renderer = "webgl";
            set_renderer("webgl");
        } catch (cause) {
            const reason = cause instanceof Error ? cause.message : String(cause);
            metrics.renderer = `canvas (${reason.slice(0, 60)})`;
            set_renderer("canvas");
        }

        fit.fit();

        let queue: Uint8Array[] = [];
        let queued_bytes = 0;
        let writing = false;
        let disposed = false;
        let frame_handle = 0;
        let last_background_flush = 0;

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

        const tick = (now: number) => {
            if (disposed) {
                return;
            }

            if (focused_ref.current) {
                flush();
            } else if (now - last_background_flush >= BACKGROUND_FLUSH_MS) {
                last_background_flush = now;
                flush();
            }

            frame_handle = requestAnimationFrame(tick);
        };

        frame_handle = requestAnimationFrame(tick);

        const flush_metrics = window.setInterval(() => {
            on_metrics(session.id, { ...metrics });
            metrics.bytes = 0;
        }, 500);

        let socket: WebSocket | null = null;

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
        observer.observe(host);

        return () => {
            disposed = true;
            cancelAnimationFrame(frame_handle);
            window.clearInterval(flush_metrics);
            observer.disconnect();
            input_disposable.dispose();
            socket?.close();
            terminal.dispose();
        };
    }, [session.id, session.kind, on_metrics]);

    const state = !stats
        ? "opening"
        : !stats.alive
          ? "exited"
          : now - stats.last_output_at <= 2
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
                focused ? "border-turquoise" : "border-reef"
            }`}
            onMouseDown={() => on_focus(session.id)}
        >
            <div className="flex shrink-0 items-center gap-2 border-b border-reef/70 px-2 py-1">
                <span className={`size-[7px] shrink-0 rounded-full ${tint}`} title={state} />
                <span className="truncate text-[12px] text-linen">{label ?? session.id}</span>
                <span className="shrink-0 rounded bg-lagoon px-1 py-[1px] font-mono text-[9px] text-shade">
                    {session.command.split(/\s+/)[0]}
                </span>

                {stats ? (
                    <span className="ml-auto flex shrink-0 items-center gap-2 font-mono text-[10px] tabular-nums text-shade">
                        {stats.context_percent !== null ? (
                            <span className="text-turquoise">{stats.context_percent}% ctx</span>
                        ) : stats.context_tokens !== null ? (
                            <span className="text-turquoise" title="what the engine reports in context">
                                {format_tokens(stats.context_tokens)} ctx
                            </span>
                        ) : null}
                        <span title={`${stats.lines.toLocaleString()} lines since start`}>
                            {format_bytes(stats.bytes)}
                        </span>
                    </span>
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

            <div ref={host_ref} className="min-h-0 flex-1 overflow-hidden p-1" />

            <div className="flex shrink-0 items-center gap-2 border-t border-reef/70 px-2 py-[3px] font-mono text-[10px] text-shade">
                <span className={state === "working" ? "text-sun" : state === "exited" ? "text-coral" : "text-shell"}>
                    {state}
                </span>
                {stats ? <span className="tabular-nums">{format_elapsed(now - stats.last_output_at)}</span> : null}
                <span className="ml-auto truncate">
                    {focused ? "live" : `${BACKGROUND_FLUSH_MS}ms`} · {renderer}
                </span>
            </div>
        </div>
    );
}
