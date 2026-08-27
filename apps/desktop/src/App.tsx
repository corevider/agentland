import { Suspense, lazy, useCallback, useEffect, useMemo, useRef, useState } from "react";

import { BoardPanel } from "@/components/BoardPanel";
import { ContextMenu, useContextMenu } from "@/components/ContextMenu";
import { Workspace } from "@/workspace/Workspace";
import {
    PANELS,
    PRESETS,
    apply_preset,
    load_layout,
    open_panel,
    preset_of,
    save_layout,
    visible_panels,
    type Layout,
    type PanelId,
} from "@/workspace/layout";
import { WorkspaceRail } from "@/workspace/WorkspaceRail";
import { WorkspaceTabs } from "@/workspace/WorkspaceTabs";
import { PreviewPanel } from "@/components/PreviewPanel";
import { SkillsPanel } from "@/components/SkillsPanel";
import { CrewPanel } from "@/components/CrewPanel";
import { RepoPanel } from "@/components/RepoPanel";
import { SettingsPage } from "@/components/SettingsPage";
import { TerminalPane, type PaneMetrics } from "@/components/TerminalPane";
import {
    is_tauri,
    kill_session,
    list_agents,
    list_tasks,
    list_workspaces,
    list_sessions,
    take_ui_commands,
    report_sample,
    spawn_generator,
    spawn_shell,
    type Agent,
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
        const sync = () =>
            list_sessions()
                .then((current) => {
                    set_sessions((held) => {
                        const live = new Map(current.map((entry) => [entry.id, entry]));
                        const kept = held.filter((entry) => live.has(entry.id)).map((entry) => live.get(entry.id)!);
                        const known = new Set(kept.map((entry) => entry.id));
                        const arrived = current.filter((entry) => !known.has(entry.id));

                        return arrived.length === 0 && kept.length === held.length ? kept : [...kept, ...arrived];
                    });
                })
                .catch((cause) => set_error(String(cause)));

        sync();
        const handle = window.setInterval(sync, 3000);
        return () => window.clearInterval(handle);
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

    const close_session = useCallback((id: string) => {
        set_sessions((held) => held.filter((entry) => entry.id !== id));
        metrics_ref.current.delete(id);
        kill_session(id).catch((cause) => set_error(String(cause)));
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
    const current_preset = useMemo(() => preset_of(layout), [layout]);
    const [workspace_id, set_workspace_id] = useState<string | null>(null);
    const [workspace_repos, set_workspace_repos] = useState<string[] | null>(null);
    const [workspace_counts, set_workspace_counts] = useState<Record<string, number>>({});
    const [rail_shut, set_rail_shut] = useState(() => {
        try {
            return localStorage.getItem("agentland-rail") === "shut";
        } catch {
            return false;
        }
    });
    const [crew, set_crew] = useState<Agent[]>([]);
    const [crew_count, set_crew_count] = useState(0);
    const [card_count, set_card_count] = useState(0);

    useEffect(() => {
        const tick = () => {
            list_agents()
                .then((roster) => {
                    set_crew(roster);
                    set_crew_count(roster.length);
                })
                .catch(() => undefined);
            list_tasks()
                .then((cards) => set_card_count(cards.filter((card) => card.column !== "done").length))
                .catch(() => undefined);
            Promise.all([list_workspaces(), list_agents()])
                .then(([listed, roster]) => {
                    const counts: Record<string, number> = {};
                    for (const workspace of listed.workspaces) {
                        counts[workspace.id] = roster.filter((agent) =>
                            workspace.repository_ids.includes(agent.repository_id),
                        ).length;
                    }
                    set_workspace_counts(counts);
                })
                .catch(() => undefined);
        };

        tick();
        const handle = window.setInterval(tick, 5000);
        return () => window.clearInterval(handle);
    }, []);
    const [zoomed_id, set_zoomed_id] = useState<string | null>(null);
    const shown_sessions = useMemo(() => {
        if (zoomed_id) {
            return sessions.filter((entry) => entry.id === zoomed_id);
        }

        if (!workspace_repos) {
            return sessions;
        }

        return sessions.filter((entry) => {
            const owner = crew.find((agent) => agent.session_id === entry.id);
            return owner ? workspace_repos.includes(owner.repository_id) : true;
        });
    }, [crew, sessions, workspace_repos, zoomed_id]);
    const grid_columns = useMemo(
        () => (shown_sessions.length > 4 ? 4 : Math.max(shown_sessions.length, 1)),
        [shown_sessions.length],
    );
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

            <header className="flex shrink-0 items-center gap-3 border-b border-reef/70 px-3 py-1.5">
                <WorkspaceTabs
                    active={workspace_id}
                    on_active={(id, repositories) => {
                        set_workspace_id(id);
                        set_workspace_repos(repositories);
                    }}
                    counts={workspace_counts}
                />

                <span className="h-4 w-px bg-reef" />

                <div className="flex items-center gap-0.5 rounded-lg border border-reef p-0.5">
                    {PRESETS.map((preset) => {
                        const active = current_preset === preset.id;
                        return (
                            <button
                                key={preset.id}
                                title={preset.hint}
                                onClick={() => set_layout(apply_preset(preset))}
                                className={`rounded px-2.5 py-[3px] text-[12px] ${
                                    active ? "bg-lagoon text-linen" : "text-shell hover:text-linen"
                                }`}
                            >
                                {preset.label}
                            </button>
                        );
                    })}
                </div>

                <div className="flex items-center gap-1">
                    <button
                        className="rounded border border-turquoise/70 px-2 py-[3px] font-mono text-[11px] text-turquoise disabled:opacity-40"
                        onClick={run_benchmark}
                        disabled={busy}
                    >
                        run benchmark
                    </button>
                    <button
                        className="rounded border border-reef px-2 py-[3px] font-mono text-[11px] text-shell hover:border-foam disabled:opacity-40"
                        onClick={open_shells}
                        disabled={busy}
                    >
                        open shells
                    </button>
                    <button
                        className="rounded border border-reef px-2 py-[3px] font-mono text-[11px] text-shell hover:border-foam"
                        onClick={clear}
                    >
                        clear
                    </button>
                </div>

                <span className="font-mono text-[10px] text-shade">
                    {pane_count} × {rate.toLocaleString()} lps
                </span>

                <div className="ml-auto flex items-center gap-3 font-mono text-[10px] tabular-nums">
                    <span className={verdict === "pass" ? "text-palm" : verdict === "marginal" ? "text-sun" : "text-coral"}>
                        {frame_stats.fps} fps
                    </span>
                    <span className="text-shade">worst {frame_stats.worst_frame_ms} ms</span>
                    <span className="text-shade">{throughput.mb_per_second} MB/s</span>
                    <span className="text-shade">
                        drop {throughput.dropped_frames} · collapsed {throughput.collapsed_mb} MB
                    </span>
                    <span className="text-shade" title={gpu.renderer}>
                        {gpu.webgl2 ? "webgl2" : gpu.renderer === "none" ? "none" : "webgl1"} · {gpu.max_contexts} ctx
                    </span>

                    <button
                        className="rounded border border-reef px-1.5 py-[3px] text-driftwood hover:border-turquoise hover:text-turquoise"
                        title="Settings"
                        aria-label="Settings"
                        onClick={() => set_settings_open(true)}
                    >
                        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
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

            <div className="flex min-h-0 min-w-0 flex-1">
                <WorkspaceRail
                    visible={visible}
                    repositories={workspace_repos}
                    counts={{ panes: sessions.length, crew: crew_count, board: card_count }}
                    collapsed={rail_shut}
                    on_collapse={(next) => {
                        set_rail_shut(next);
                        try {
                            localStorage.setItem("agentland-rail", next ? "shut" : "open");
                        } catch {
                            // a rail that cannot remember its state is still a rail
                        }
                    }}
                    on_open_panel={focus_panel}
                    on_open_agent={(agent) => {
                        if (agent.session_id) {
                            void open_session(agent.session_id);
                        } else {
                            focus_panel("island");
                        }
                    }}
                    footer={
                        <div className="flex items-center justify-between font-mono text-[10px] text-shade">
                            <span>{sessions.length} panes</span>
                            <span className={verdict === "pass" ? "text-palm" : verdict === "marginal" ? "text-sun" : "text-coral"}>
                                {frame_stats.fps} fps
                            </span>
                        </div>
                    }
                />

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
                        return <BoardPanel active repositories={workspace_repos} />;
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
                            className="grid min-h-0 min-w-0 flex-1 gap-1.5 p-1.5"
                            style={{
                                gridTemplateColumns: `repeat(${grid_columns}, minmax(0, 1fr))`,
                                gridAutoRows: "minmax(0, 1fr)",
                            }}
                        >
                            {shown_sessions.map((session) => (
                                <TerminalPane
                                    key={session.id}
                                    session={session}
                                    label={
                                        crew.find((agent) => agent.session_id === session.id)?.name
                                    }
                                    focused={
                                        focused_id ? focused_id === session.id : session.id === sessions[0]?.id
                                    }
                                    on_focus={set_focused_id}
                                    on_close={close_session}
                                    on_zoom={(id) => set_zoomed_id((held) => (held === id ? null : id))}
                                    zoomed={zoomed_id === session.id}
                                    on_branch={(entry) => {
                                        if (!entry.cwd) {
                                            return;
                                        }
                                        spawn_shell("bash", entry.cwd)
                                            .then((created) => {
                                                set_sessions((held) => [...held, created]);
                                                set_focused_id(created.id);
                                            })
                                            .catch((cause) => set_error(String(cause)));
                                    }}
                                    on_metrics={on_metrics}
                                />
                            ))}
                        </main>
                    );
                }}
                subtitle_for={(panel: PanelId) => {
                    if (panel === "panes") {
                        return shown_sessions.length === sessions.length
                            ? `${sessions.length} open`
                            : `${shown_sessions.length} of ${sessions.length}`;
                    }
                    if (panel === "island") {
                        return `${pane_count} × ${rate.toLocaleString()} lps`;
                    }
                    return undefined;
                }}
            />
            </div>

        </div>
    );
}
