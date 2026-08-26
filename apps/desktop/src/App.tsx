import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { CrewPanel } from "@/components/CrewPanel";
import { RepoPanel } from "@/components/RepoPanel";
import { SettingsPage } from "@/components/SettingsPage";
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
import { load_settings, save_settings, type Settings } from "@/lib/settings";

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
    const [settings, set_settings] = useState<Settings>(() => load_settings());
    const [settings_open, set_settings_open] = useState(false);
    const [busy, set_busy] = useState(false);
    const [error, set_error] = useState<string | null>(null);
    const metrics_ref = useRef(new Map<string, PaneMetrics>());
    const run_ref = useRef<{ id: string; started: number; panes: number; rate: number } | null>(null);
    const frame_ref = useRef({ fps: 0, worst_frame_ms: 0 });
    const [focused_id, set_focused_id] = useState<string | null>(null);
    const [view, set_view] = useState<"panes" | "repos" | "crew">("panes");
    const pane_count = settings.panes;
    const rate = settings.lines_per_second;
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

    const open_session = useCallback(async (session_id: string) => {
        const current = await list_sessions();
        const session = current.find((entry) => entry.id === session_id);
        if (!session) {
            set_error(`session ${session_id} is gone`);
            return;
        }

        set_sessions((existing) =>
            existing.some((entry) => entry.id === session_id) ? existing : [...existing, session],
        );
        set_focused_id(session_id);
        set_view("panes");
    }, []);

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
    }, [clear, pane_count, rate, settings.duration_ms]);

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

    const update_settings = useCallback((next: Settings) => {
        set_settings(next);
        save_settings(next);
    }, []);

    return (
        <div className="relative flex h-screen flex-col bg-[#0b1113] text-[#d6e2e6]">
            {settings_open ? (
                <SettingsPage
                    settings={settings}
                    on_change={update_settings}
                    on_close={() => set_settings_open(false)}
                    gpu={gpu}
                    surface={detect_surface()}
                />
            ) : null}

            <header className="flex flex-wrap items-center gap-4 border-b border-[#26343a] px-4 py-3">
                <span className="font-mono text-xs uppercase tracking-[0.14em] text-[#45bcc4]">Agentland</span>

                <div className="flex">
                    {(["panes", "repos", "crew"] as const).map((choice) => (
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

                <span className="font-mono text-[11px] text-[#7b8d94]">
                    {pane_count} × {rate.toLocaleString()} lps
                </span>

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

                    <button
                        className="border border-[#26343a] px-2 py-1 text-[#a4b5bb] hover:border-[#45bcc4] hover:text-[#45bcc4]"
                        title="Settings"
                        aria-label="Settings"
                        onClick={() => set_settings_open(true)}
                    >
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                            <circle cx="12" cy="12" r="3" />
                            <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.6 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.6a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
                        </svg>
                    </button>
                </div>
            </header>

            {error ? (
                <div className="border-b border-[#d46969] bg-[#1b1113] px-4 py-2 font-mono text-xs text-[#d46969]">
                    {error}
                </div>
            ) : null}

            {view === "repos" ? <RepoPanel /> : null}
            {view === "crew" ? <CrewPanel on_open_session={open_session} /> : null}

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
