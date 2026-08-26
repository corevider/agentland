import { Suspense, lazy, useCallback, useEffect, useMemo, useRef, useState } from "react";

import { BoardPanel } from "@/components/BoardPanel";
import { ContextMenu, useContextMenu } from "@/components/ContextMenu";
import { Workspace } from "@/workspace/Workspace";
import {
    PANELS,
    load_layout,
    open_panel,
    save_layout,
    visible_panels,
    type Layout,
    type PanelId,
} from "@/workspace/layout";
import { PreviewPanel } from "@/components/PreviewPanel";
import { SkillsPanel } from "@/components/SkillsPanel";
import { CrewPanel } from "@/components/CrewPanel";
import { RepoPanel } from "@/components/RepoPanel";
import { SettingsPage } from "@/components/SettingsPage";
import { TerminalPane, type PaneMetrics } from "@/components/TerminalPane";
import {
    is_tauri,
    kill_session,
    list_sessions,
    take_ui_commands,
    report_sample,
    spawn_generator,
    spawn_shell,
    type SessionInfo,
} from "@/lib/core";
import { island_frames } from "@/lib/frames";
import { probe_gpu, type GpuReport } from "@/lib/gpu";
import { load_settings, save_settings, type Settings } from "@/lib/settings";

const IslandPanel = lazy(() =>
    import("@/components/IslandPanel").then((module) => ({ default: module.IslandPanel })),
);

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
    const island_ref = useRef({ rendered: 0, at: 0 });
    const [focused_id, set_focused_id] = useState<string | null>(null);
    const [layout, set_layout_state] = useState<Layout>(() => load_layout());
    const layout_ref = useRef(layout);
    layout_ref.current = layout;
    const pane_count = settings.panes;
    const rate = settings.lines_per_second;
    const [throughput, set_throughput] = useState({ mb_per_second: 0, dropped_frames: 0, collapsed_mb: 0 });
    const frame_stats = use_frame_stats();
    frame_ref.current = frame_stats;
    const [gpu] = useState<GpuReport>(() => probe_gpu());
    const menu = useContextMenu();


    useEffect(() => {
        list_sessions().then(set_sessions).catch((cause) => set_error(String(cause)));
    }, []);

    const set_layout = useCallback((next: Layout) => {
        set_layout_state(next);
        save_layout(next);
    }, []);

    const focus_panel = useCallback((panel: PanelId) => {
        set_layout_state((current) => {
            const next = open_panel(current, panel);
            save_layout(next);
            return next;
        });
    }, []);

    useEffect(() => {
        const handle = window.setInterval(() => {
            take_ui_commands()
                .then((commands) => {
                    for (const command of commands) {
                        if (command.startsWith("view:")) {
                            const target = command.slice("view:".length);
                            if (PANELS.some((panel) => panel.id === target)) {
                                focus_panel(target as PanelId);
                            }
                            continue;
                        }

                        window.dispatchEvent(
                            new CustomEvent("agentland:command", { detail: command }),
                        );
                    }
                })
                .catch(() => undefined);
        }, 1500);

        return () => window.clearInterval(handle);
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

            const now = performance.now();
            const previous = island_ref.current;
            const seconds = previous.at > 0 ? (now - previous.at) / 1000 : 0;
            const island_fps =
                seconds > 0
                    ? Math.round((island_frames.rendered - previous.rendered) / seconds)
                    : 0;
            const island_worst_ms = Math.round(island_frames.worst_ms);
            island_frames.worst_ms = 0;
            island_ref.current = { rendered: island_frames.rendered, at: now };

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
                island_fps,
                island_worst_ms,
                panels: visible_panels(layout_ref.current).join("+"),
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
        focus_panel("panes");
    }, [focus_panel]);

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

    useEffect(() => {
        const listener = (event: Event) => {
            const command = (event as CustomEvent<string>).detail;
            if (!command.startsWith("bench:")) {
                return;
            }

            const wants_island = command === "bench:with-island";
            set_layout({
                ...layout_ref.current,
                slots: {
                    left_top: { panels: wants_island ? ["island"] : ["board"], active: 0 },
                    left_bottom: { panels: [], active: 0 },
                    right_top: { panels: ["panes"], active: 0 },
                    right_bottom: { panels: [], active: 0 },
                },
                column_fraction: wants_island ? 0.38 : 0.2,
            });

            window.setTimeout(() => void run_benchmark(), 600);
        };

        window.addEventListener("agentland:command", listener);
        return () => window.removeEventListener("agentland:command", listener);
    }, [run_benchmark, set_layout]);

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

    const visible = useMemo(() => visible_panels(layout), [layout]);
    const grid_columns = useMemo(() => (sessions.length > 4 ? 4 : Math.max(sessions.length, 1)), [sessions.length]);
    const verdict = frame_stats.fps >= 55 ? "pass" : frame_stats.fps >= 30 ? "marginal" : "fail";

    const update_settings = useCallback((next: Settings) => {
        set_settings(next);
        save_settings(next);
    }, []);

    const open_window_menu = useCallback(
        (event: React.MouseEvent) => {
            menu.open(event, "Agentland", [
                { label: "Island", hint: "panel", run: () => focus_panel("island") },
                { label: "Terminals", hint: "panel", run: () => focus_panel("panes") },
                { label: "Board", hint: "panel", run: () => focus_panel("board") },
                { label: "Repositories", hint: "panel", run: () => focus_panel("repos") },
                { label: "Crew", hint: "panel", run: () => focus_panel("crew") },
                { label: "Skills", hint: "panel", run: () => focus_panel("skills") },
                { label: "Preview", hint: "panel", run: () => focus_panel("preview") },
                {
                    label: "Capture the island",
                    hint: "png",
                    disabled: !visible_panels(layout).includes("island"),
                    run: () => {
                        window.dispatchEvent(new CustomEvent("agentland:capture-island"));
                    },
                },
                { label: "Settings", run: () => set_settings_open(true) },
                {
                    label: "Reload the interface",
                    run: () => window.location.reload(),
                },
                ...(import.meta.env.DEV
                    ? [
                          {
                              label: "Developer tools",
                              hint: "dev",
                              run: () => {
                                  void import("@tauri-apps/api/webviewWindow")
                                      .then((module) => {
                                          const window_handle = module.getCurrentWebviewWindow() as unknown as {
                                              internalToggleDevtools?: () => void;
                                          };
                                          window_handle.internalToggleDevtools?.();
                                      })
                                      .catch(() => undefined);
                              },
                          },
                      ]
                    : []),
            ]);
        },
        [menu, layout, focus_panel],
    );

    return (
        <div
            className="relative flex h-screen flex-col bg-lagoon-deep text-linen"
            onContextMenu={open_window_menu}
        >
            <ContextMenu request={menu.request} on_close={menu.close} />
            {settings_open ? (
                <SettingsPage
                    settings={settings}
                    on_change={update_settings}
                    on_close={() => set_settings_open(false)}
                    gpu={gpu}
                    surface={detect_surface()}
                />
            ) : null}

            <header className="flex flex-wrap items-center gap-4 border-b border-reef/70 px-5 py-3">
                <span className="font-display text-[19px] font-semibold tracking-tight text-linen">
                    Agentland
                </span>

                <div className="flex gap-1">
                    {PANELS.map((panel) => {
                        const shown = visible.includes(panel.id);

                        return (
                            <button
                                key={panel.id}
                                title={panel.hint}
                                className={`rounded-lg border px-3 py-1 font-mono text-xs ${
                                    shown ? "border-turquoise text-turquoise" : "border-reef text-shell"
                                }`}
                                onClick={() => focus_panel(panel.id)}
                            >
                                {panel.label.toLowerCase()}
                            </button>
                        );
                    })}
                </div>

                <button
                    className="border border-turquoise px-3 py-1 font-mono text-xs text-turquoise disabled:opacity-40 rounded-lg"
                    onClick={run_benchmark}
                    disabled={busy}
                >
                    run benchmark
                </button>

                <button
                    className="border border-foam px-3 py-1 font-mono text-xs disabled:opacity-40 rounded-lg"
                    onClick={open_shells}
                    disabled={busy}
                >
                    open shells
                </button>

                <button className="border border-foam px-3 py-1 font-mono text-xs rounded-lg" onClick={clear}>
                    clear
                </button>

                <span className="font-mono text-[11px] text-shell">
                    {pane_count} × {rate.toLocaleString()} lps
                </span>

                <div className="ml-auto flex items-center gap-5 font-mono text-xs tabular-nums">
                    <span className={verdict === "pass" ? "text-palm" : verdict === "marginal" ? "text-sun" : "text-coral"}>
                        {frame_stats.fps} fps
                    </span>
                    <span className="text-shell">worst {frame_stats.worst_frame_ms} ms</span>
                    <span className="text-shell">{throughput.mb_per_second} MB/s</span>
                    <span className="text-shell">
                        core drop {throughput.dropped_frames} · collapsed {throughput.collapsed_mb} MB
                    </span>
                    <span className="text-shell" title={gpu.renderer}>
                        gpu {gpu.webgl2 ? "webgl2" : gpu.renderer === "none" ? "none" : "webgl1"} · {gpu.max_contexts} ctx
                    </span>

                    <button
                        className="border border-reef px-2 py-1 text-driftwood hover:border-turquoise hover:text-turquoise rounded-lg"
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
                <div className="border-b border-coral bg-lagoon px-4 py-2 font-mono text-xs text-coral">
                    {error}
                </div>
            ) : null}

            <Workspace
                layout={layout}
                on_layout={set_layout}
                render_panel={(panel: PanelId, active: boolean) => {
                    if (panel === "repos") {
                        return <RepoPanel active />;
                    }
                    if (panel === "preview") {
                        return <PreviewPanel active={active} />;
                    }
                    if (panel === "skills") {
                        return <SkillsPanel active />;
                    }
                    if (panel === "crew") {
                        return <CrewPanel active on_open_session={open_session} />;
                    }
                    if (panel === "board") {
                        return <BoardPanel active />;
                    }
                    if (panel === "island") {
                        return (
                            <Suspense
                                fallback={
                                    <div className="flex min-h-0 flex-1 items-center justify-center font-mono text-xs text-shell">
                                        loading the island…
                                    </div>
                                }
                            >
                                <IslandPanel active on_open_session={open_session} />
                            </Suspense>
                        );
                    }

                    return (
                        <main
                            className="grid min-h-0 min-w-0 flex-1 gap-2 p-2"
                            style={{
                                gridTemplateColumns: `repeat(${grid_columns}, minmax(0, 1fr))`,
                                gridAutoRows: "minmax(0, 1fr)",
                            }}
                        >
                            {sessions.map((session) => (
                                <TerminalPane
                                    key={session.id}
                                    session={session}
                                    focused={
                                        focused_id ? focused_id === session.id : session.id === sessions[0]?.id
                                    }
                                    on_focus={set_focused_id}
                                    on_metrics={on_metrics}
                                />
                            ))}
                        </main>
                    );
                }}
                subtitle_for={(panel: PanelId) => {
                    if (panel === "panes") {
                        return `${sessions.length} open`;
                    }
                    if (panel === "island") {
                        return `${pane_count} × ${rate.toLocaleString()} lps`;
                    }
                    return undefined;
                }}
            />

        </div>
    );
}
