import { useEffect, useRef, useState } from "react";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";

import { open_stream, resize_session, write_input, type SessionInfo } from "@/lib/core";

const MAX_PENDING_WRITES = 8;

export interface PaneMetrics {
    bytes: number;
    dropped_frames: number;
    dropped_local: number;
    renderer: string;
}

interface Props {
    session: SessionInfo;
    on_metrics: (id: string, metrics: PaneMetrics) => void;
}

export function TerminalPane({ session, on_metrics }: Props) {
    const host_ref = useRef<HTMLDivElement>(null);
    const [renderer, set_renderer] = useState("canvas");

    useEffect(() => {
        const host = host_ref.current;
        if (!host) {
            return;
        }

        const terminal = new Terminal({
            fontSize: 12,
            fontFamily: "'IBM Plex Mono', ui-monospace, monospace",
            scrollback: 2000,
            theme: { background: "#0d1315", foreground: "#d6e2e6" },
        });

        const metrics: PaneMetrics = { bytes: 0, dropped_frames: 0, dropped_local: 0, renderer: "canvas" };

        const fit = new FitAddon();
        terminal.loadAddon(fit);
        terminal.open(host);

        try {
            const webgl = new WebglAddon();
            webgl.onContextLoss(() => webgl.dispose());
            terminal.loadAddon(webgl);
            metrics.renderer = "webgl";
            set_renderer("webgl");
        } catch {
            set_renderer("canvas");
        }

        fit.fit();

        let pending_writes = 0;
        let disposed = false;

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

                if (pending_writes >= MAX_PENDING_WRITES) {
                    metrics.dropped_local += 1;
                    return;
                }

                pending_writes += 1;
                terminal.write(payload, () => {
                    pending_writes -= 1;
                });
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
            window.clearInterval(flush_metrics);
            observer.disconnect();
            input_disposable.dispose();
            socket?.close();
            terminal.dispose();
        };
    }, [session.id, session.kind, on_metrics]);

    return (
        <div className="flex min-h-0 flex-col border border-[#26343a] bg-[#0d1315]">
            <div className="flex items-center justify-between border-b border-[#26343a] px-2 py-1 font-mono text-[11px] text-[#7b8d94]">
                <span>{session.id}</span>
                <span>{renderer}</span>
            </div>
            <div ref={host_ref} className="min-h-0 flex-1 overflow-hidden p-1" />
        </div>
    );
}
