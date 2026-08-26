import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { RepoPanel } from "@/components/RepoPanel";
import { TerminalPane, type PaneMetrics } from "@/components/TerminalPane";
import {
    is_tauri,
    kill_session,
    list_sessions,
    report_sample,
    spawn_generator,
    spawn_shell,
    type SessionInfo,
} from "@/lib/core";
import { probe_gpu, type GpuReport } from "@/lib/gpu";

const PANE_CHOICES = [1, 4, 8];
const RATE_CHOICES = [1_000, 5_000, 10_000, 20_000];

function detect_surface(): string {
    const agent = navigator.userAgent;
    if (is_tauri()) {
        return agent.includes("WebKit") && !agent.includes("Chrome") ? "tauri-webkitgtk" : "tauri-webview";
    }
    if (agent.includes("Firefox")) {
        return "firefox";
    }
    if (agent.includes("Chrome") || agent.includes("Chromium")) {
        return "chromium";
    }
    return "webkit";
}

interface FrameStats {
    fps: number;
    worst_frame_ms: number;
}

function use_frame_stats(): FrameStats {
    const [stats, set_stats] = useState<FrameStats>({ fps: 0, worst_frame_ms: 0 });

    useEffect(() => {
        let frames = 0;
        let worst = 0;
        let last = performance.now();
        let window_start = last;
        let handle = 0;

        const tick = (now: number) => {
            const delta = now - last;
            last = now;
            frames += 1;
            worst = Math.max(worst, delta);

            if (now - window_start >= 500) {
                set_stats({
                    fps: Math.round((frames * 1000) / (now - window_start)),
                    worst_frame_ms: Math.round(worst),
                });
                frames = 0;
                worst = 0;
                window_start = now;
            }

            handle = requestAnimationFrame(tick);
        };

        handle = requestAnimationFrame(tick);
        return () => cancelAnimationFrame(handle);
    }, []);

    return stats;
}

export default function App() {
    const [sessions, set_sessions] = useState<SessionInfo[]>([]);
    const [pane_count, set_pane_count] = useState(8);
    const [rate, set_rate] = useState(10_000);
    const [busy, set_busy] = useState(false);
    const [error, set_error] = useState<string | null>(null);
    const metrics_ref = useRef(new Map<string, PaneMetrics>());
    const run_ref = useRef<{ id: string; started: number; panes: number; rate: number } | null>(null);
    const frame_ref = useRef({ fps: 0, worst_frame_ms: 0 });
    const [focused_id, set_focused_id] = useState<string | null>(null);
    const [view, set_view] = useState<"panes" | "repos">("panes");
    const [throughput, set_throughput] = useState({ mb_per_second: 0, dropped_frames: 0, collapsed_mb: 0 });
    const frame_stats = use_frame_stats();
    frame_ref.current = frame_stats;
    const [gpu] = useState<GpuReport>(() => probe_gpu());

    useEffect(() => {
        list_sessions().then(set_sessions).catch((cause) => set_error(String(cause)));
    }, []);

    useEffect(() => {
        const handle = window.setInterval(() => {
            let bytes = 0;
            let dropped_frames = 0;
            let collapsed_bytes = 0;

            for (const entry of metrics_ref.current.values()) {
                bytes += entry.bytes;
                dropped_frames += entry.dropped_frames;
                collapsed_bytes += entry.collapsed_bytes;
            }

            set_throughput({
                mb_per_second: Number(((bytes * 2) / (1024 * 1024)).toFixed(2)),
                dropped_frames,
                collapsed_mb: Number((collapsed_bytes / (1024 * 1024)).toFixed(1)),
            });
        }, 500);

        return () => window.clearInterval(handle);
    }, []);

    const on_metrics = useCallback((id: string, value: PaneMetrics) => {
        metrics_ref.current.set(id, value);
    }, []);

    useEffect(() => {
        const handle = window.setInterval(() => {
            const run = run_ref.current;
            if (!run) {
                return;
            }

            let bytes = 0;
            let dropped_frames = 0;
            let collapsed_bytes = 0;
            let renderer = "canvas";

            for (const entry of metrics_ref.current.values()) {
                bytes += entry.bytes;
                dropped_frames += entry.dropped_frames;
                collapsed_bytes += entry.collapsed_bytes;
                renderer = entry.renderer;
            }

            void report_sample({
                run_id: run.id,
                elapsed_ms: Math.round(performance.now() - run.started),
                panes: run.panes,
                lines_per_second: run.rate,
                fps: frame_ref.current.fps,
                worst_frame_ms: frame_ref.current.worst_frame_ms,
                mb_per_second: Number(((bytes * 2) / (1024 * 1024)).toFixed(2)),
                dropped_frames,
                dropped_local: collapsed_bytes,
                renderer,
                surface: detect_surface(),
                gpu: `${gpu.webgl2 ? "webgl2" : gpu.renderer === "none" ? "none" : "webgl1"} · ${gpu.renderer} · ${gpu.max_contexts} ctx`,
            }).catch(() => undefined);
        }, 2000);

        return () => window.clearInterval(handle);
    }, [gpu]);

    const clear = useCallback(async () => {
        const current = await list_sessions();
        await Promise.all(current.map((session) => kill_session(session.id).catch(() => undefined)));
        metrics_ref.current.clear();
        run_ref.current = null;
        set_sessions([]);
    }, []);

    const run_benchmark = useCallback(async () => {
        set_busy(true);
        set_error(null);
        try {
            await clear();
            const created = await Promise.all(
                Array.from({ length: pane_count }, () =>
                    spawn_generator({
                        lines_per_second: rate,
                        duration_ms: 30_000,
                        line_width: 96,
                        colored: true,
                    }),
                ),
            );
            run_ref.current = { id: `run-${Date.now()}`, started: performance.now(), panes: pane_count, rate };
            set_focused_id(created[0]?.id ?? null);
            set_sessions(created);
        } catch (cause) {
            set_error(String(cause));
        } finally {
            set_busy(false);
        }
    }, [clear, pane_count, rate]);

    const open_shells = useCallback(async () => {
        set_busy(true);
        set_error(null);
        try {
            await clear();
            const created = await Promise.all(
                Array.from({ length: pane_count }, () => spawn_shell("bash")),
            );
            set_sessions(created);
        } catch (cause) {
            set_error(String(cause));
        } finally {
            set_busy(false);
        }
    }, [clear, pane_count]);

    const grid_columns = useMemo(() => (sessions.length > 4 ? 4 : Math.max(sessions.length, 1)), [sessions.length]);
    const verdict = frame_stats.fps >= 55 ? "pass" : frame_stats.fps >= 30 ? "marginal" : "fail";

    return (
        <div className="flex h-screen flex-col bg-[#0b1113] text-[#d6e2e6]">
            <header className="flex flex-wrap items-center gap-4 border-b border-[#26343a] px-4 py-3">
                <span className="font-mono text-xs uppercase tracking-[0.14em] text-[#45bcc4]">Agentland</span>

                <div className="flex">
                    {(["panes", "repos"] as const).map((choice) => (
                        <button
                            key={choice}
                            className={`border px-3 py-1 font-mono text-xs ${
                                view === choice
                                    ? "border-[#45bcc4] text-[#45bcc4]"
                                    : "border-[#26343a] text-[#7b8d94]"
                            }`}
                            onClick={() => set_view(choice)}
                        >
                            {choice}
                        </button>
                    ))}
                </div>

                <label className="flex items-center gap-2 text-xs">
                    panes
                    <select
                        className="border border-[#26343a] bg-[#141c1f] px-2 py-1 font-mono"
                        value={pane_count}
                        onChange={(event) => set_pane_count(Number(event.target.value))}
                    >
                        {PANE_CHOICES.map((choice) => (
                            <option key={choice} value={choice}>
                                {choice}
                            </option>
                        ))}
                    </select>
                </label>

                <label className="flex items-center gap-2 text-xs">
                    lines/sec each
                    <select
                        className="border border-[#26343a] bg-[#141c1f] px-2 py-1 font-mono"
                        value={rate}
                        onChange={(event) => set_rate(Number(event.target.value))}
                    >
                        {RATE_CHOICES.map((choice) => (
                            <option key={choice} value={choice}>
                                {choice.toLocaleString()}
                            </option>
                        ))}
                    </select>
                </label>

                <button
                    className="border border-[#45bcc4] px-3 py-1 font-mono text-xs text-[#45bcc4] disabled:opacity-40"
                    onClick={run_benchmark}
                    disabled={busy}
                >
                    run benchmark
                </button>

                <button
                    className="border border-[#3a4d55] px-3 py-1 font-mono text-xs disabled:opacity-40"
                    onClick={open_shells}
                    disabled={busy}
                >
                    open shells
                </button>

                <button className="border border-[#3a4d55] px-3 py-1 font-mono text-xs" onClick={clear}>
                    clear
                </button>

                <div className="ml-auto flex items-center gap-5 font-mono text-xs tabular-nums">
                    <span className={verdict === "pass" ? "text-[#5aa87c]" : verdict === "marginal" ? "text-[#c99a2e]" : "text-[#d46969]"}>
                        {frame_stats.fps} fps
                    </span>
                    <span className="text-[#7b8d94]">worst {frame_stats.worst_frame_ms} ms</span>
                    <span className="text-[#7b8d94]">{throughput.mb_per_second} MB/s</span>
                    <span className="text-[#7b8d94]">
                        core drop {throughput.dropped_frames} · collapsed {throughput.collapsed_mb} MB
                    </span>
                    <span className="text-[#7b8d94]" title={gpu.renderer}>
                        gpu {gpu.webgl2 ? "webgl2" : gpu.renderer === "none" ? "none" : "webgl1"} · {gpu.max_contexts} ctx
                    </span>
                </div>
            </header>

            {error ? (
                <div className="border-b border-[#d46969] bg-[#1b1113] px-4 py-2 font-mono text-xs text-[#d46969]">
                    {error}
                </div>
            ) : null}

            {view === "repos" ? <RepoPanel /> : null}

            <main
                hidden={view !== "panes"}
                className="grid min-h-0 flex-1 gap-2 p-2"
                style={{
                    gridTemplateColumns: `repeat(${grid_columns}, minmax(0, 1fr))`,
                    gridAutoRows: "minmax(0, 1fr)",
                }}
            >
                {sessions.map((session) => (
                    <TerminalPane
                        key={session.id}
                        session={session}
                        focused={focused_id ? focused_id === session.id : session.id === sessions[0]?.id}
                        on_focus={set_focused_id}
                        on_metrics={on_metrics}
                    />
                ))}
            </main>
        </div>
    );
}
