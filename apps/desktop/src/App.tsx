import { Suspense, lazy, useCallback, useEffect, useMemo, useRef, useState } from "react";

import { BoardPanel } from "@/components/BoardPanel";
import { ContextMenu, useContextMenu } from "@/components/ContextMenu";
import { Workspace } from "@/workspace/Workspace";
import {
    focus_panel as focus_panel_in,
    load_layout,
    save_layout,
    visible_panels,
    type Layout,
    type PanelId,
    stacks,
} from "@/workspace/layout";
import { PANELS, ServiceProvider, is_known_panel, type WorkspaceServices } from "@/workspace/registry";
import { PRESETS, preset_of } from "@/workspace/presets";
import { bench_layout } from "@/workspace/bench_layout";
import { WorkspaceRail } from "@/workspace/WorkspaceRail";
import { WorkspaceTabs } from "@/workspace/WorkspaceTabs";
import { PreviewPanel } from "@/components/PreviewPanel";
import { SkillsPanel } from "@/components/SkillsPanel";
import { CrewPanel } from "@/components/CrewPanel";
import { RepoPanel } from "@/components/RepoPanel";
import { NoticeBell } from "@/components/NoticeBell";
import { Jumper, PlaceTrail } from "@/components/Jumper";
import { SettingsPage } from "@/components/SettingsPage";
import { TerminalPane, type PaneMetrics } from "@/components/TerminalPane";
import {
    is_tauri,
    kill_session,
    list_agents,
    list_repos,
    list_tasks,
    list_workspaces,
    list_sessions,
    take_ui_commands,
    report_sample,
    spawn_generator,
    spawn_default_shell,
    type Agent,
    type SessionInfo,
    voice_state,
    write_input,
} from "@/lib/core";
import { island_frames } from "@/lib/frames";
import { can_listen } from "@/lib/listen";
import { begin_speaking, end_speaking } from "@/lib/speaking";
import { probe_gpu, type GpuReport } from "@/lib/gpu";
import { load_settings, save_settings, type Settings } from "@/lib/settings";
import { detect_surface } from "@/lib/surface";

const IslandPanel = lazy(() =>
    import("@/components/IslandPanel").then((module) => ({ default: module.IslandPanel })),
);


interface FrameStats {
    fps: number;
    worst_frame_ms: number;
}

/// Measuring smoothness costs smoothness. A rAF loop that never sleeps cost 8 of
/// this app's 23 idle CPU points on this machine, and sampling it in bursts still
/// cost 3 — WebKit keeps the frame pipeline warm once anything asks for a frame.
/// So at rest nothing asks: the reading comes from the frames the island already
/// counts, and the real meter only runs while a benchmark needs a true number.
function use_frame_stats(measuring: boolean): FrameStats {
    const [stats, set_stats] = useState<FrameStats>({ fps: 0, worst_frame_ms: 0 });

    useEffect(() => {
        if (measuring) {
            let frames = 0;
            let worst = 0;
            let last = performance.now();
            let window_start = last;
            let handle = 0;

            const tick = (now: number) => {
                worst = Math.max(worst, now - last);
                last = now;
                frames += 1;

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
        }

        let seen = island_frames.rendered;
        let at = performance.now();

        const read = () => {
            const now = performance.now();
            const seconds = (now - at) / 1000;
            const drawn = island_frames.rendered - seen;
            seen = island_frames.rendered;
            at = now;

            const fresh = {
                fps: seconds > 0 ? Math.round(drawn / seconds) : 0,
                worst_frame_ms: Math.round(island_frames.worst_ms),
            };

            // A new object every second re-renders every panel under it, and the
            // island redraws with them. Only a changed reading is worth that.
            set_stats((held) =>
                held.fps === fresh.fps && held.worst_frame_ms === fresh.worst_frame_ms ? held : fresh,
            );
        };

        const handle = window.setInterval(read, 1000);
        return () => window.clearInterval(handle);
    }, [measuring]);

    return stats;
}

export default function App() {
    const [sessions, set_sessions] = useState<SessionInfo[]>([]);
    const [settings, set_settings] = useState<Settings>(() => load_settings());
    const [settings_open, set_settings_open] = useState(false);
    /// Held down, the microphone is recording; let go, what was said is typed
    /// into the pane being watched. Not sent — read it before you send it.
    const [listening, set_listening] = useState(false);
    /// Reading back what was said takes a second or two on this machine, and a
    /// button that looks idle while it works reads as a button that did
    /// nothing.
    const [reading, set_reading] = useState(false);
    const [heard, set_heard] = useState<string | null>(null);
    const [busy, set_busy] = useState(false);
    const [error, set_error] = useState<string | null>(null);
    const [can_record, set_can_record] = useState(true);
    const metrics_ref = useRef(new Map<string, PaneMetrics>());
    const run_ref = useRef<{ id: string; started: number; panes: number; rate: number } | null>(null);
    const frame_ref = useRef({ fps: 0, worst_frame_ms: 0 });
    const island_ref = useRef({ rendered: 0, at: 0 });
    const [focused_id, set_focused_id] = useState<string | null>(null);
    const [update_out, set_update_out] = useState<string | null>(null);
    const [layout, set_layout_state] = useState<Layout>(() => load_layout(is_known_panel));
    const layout_ref = useRef(layout);
    layout_ref.current = layout;
    const pane_count = settings.panes;
    const rate = settings.lines_per_second;
    const [throughput, set_throughput] = useState({ mb_per_second: 0, dropped_frames: 0, collapsed_mb: 0 });
    const [measuring, set_measuring] = useState(false);
    const frame_stats = use_frame_stats(measuring);
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
                        const arrived = current.filter(
                            (entry) => !known.has(entry.id) && !put_away.current.has(entry.id),
                        );

                        return arrived.length === 0 && kept.length === held.length ? kept : [...kept, ...arrived];
                    });
                })
                .catch((cause) => set_error(String(cause)));

        sync();
        const handle = window.setInterval(sync, 3000);
        return () => window.clearInterval(handle);
    }, []);

    // Whether anything on this machine can record. Asked once: a recorder is
    // installed by a person, not by the app, so it does not change under us.
    useEffect(() => {
        if (can_listen()) {
            set_can_record(true);
            return;
        }

        voice_state()
            .then((state) => set_can_record(Boolean(state.recorder)))
            .catch(() => undefined);
    }, []);

    const set_layout = useCallback((next: Layout) => {
        set_layout_state(next);
        save_layout(next);
    }, []);

    const focus_panel = useCallback((panel: PanelId) => {
        set_layout_state((current) => {
            const next = focus_panel_in(current, panel);
            save_layout(next);
            return next;
        });
    }, []);

    // Asked once, quietly, when the window opens. Finding an update is worth
    // saying; taking one is a decision, and it stays a person's — nothing is
    // downloaded or replaced until they press the button in Settings.
    useEffect(() => {
        if (!is_tauri()) {
            return;
        }

        let cancelled = false;
        import("@tauri-apps/plugin-updater")
            .then((api) => api.check())
            .then((found) => {
                if (!cancelled && found) {
                    set_update_out(found.version);
                }
            })
            .catch(() => undefined);

        return () => {
            cancelled = true;
        };
    }, []);

    // Nothing is open on a fresh install, and every panel below assumes a
    // project. The one that makes one comes forward instead of leaving a person
    // to guess which of a dozen tabs comes first.
    useEffect(() => {
        list_repos()
            .then((known) => {
                if (known.length === 0) {
                    focus_panel("start");
                }
            })
            .catch(() => undefined);
    }, [focus_panel]);

    const go_and_see_ref = useRef<(opens: string) => void>(() => undefined);

    useEffect(() => {
        const handle = window.setInterval(() => {
            take_ui_commands()
                .then((commands) => {
                    for (const command of commands) {
                        if (command.startsWith("preset:")) {
                            const wanted = command.slice("preset:".length);
                            const preset = PRESETS.find((entry) => entry.id === wanted);
                            if (preset) {
                                set_layout(preset.build());
                            }
                            continue;
                        }

                        if (command === "jump") {
                            set_jumping(true);
                            continue;
                        }

                        if (command === "reload") {
                            window.location.reload();
                            continue;
                        }

                        // One panel, the whole window. The board's later
                        // columns sit off the edge in every shared layout, so
                        // there was no way to be shown them from outside.
                        if (command.startsWith("only:")) {
                            const target = command.slice("only:".length);
                            if (PANELS.some((panel) => panel.id === target)) {
                                set_layout_state((current) => {
                                    const shown = focus_panel_in(current, target as PanelId);
                                    const holding = stacks(shown.root).find((stack) =>
                                        stack.tabs.some((tab) => tab.panel === target),
                                    );
                                    const next = holding
                                        ? { ...shown, maximised: holding.id }
                                        : shown;
                                    save_layout(next);
                                    return next;
                                });
                            }
                            continue;
                        }

                        if (command.startsWith("view:")) {
                            const target = command.slice("view:".length);
                            if (PANELS.some((panel) => panel.id === target)) {
                                focus_panel(target as PanelId);
                            }
                            continue;
                        }

                        // From the tray: somebody needs a person, and this
                        // is the line they clicked to come and see.
                        if (command.startsWith("open:")) {
                            go_and_see_ref.current(command.slice("open:".length));
                            continue;
                        }

                        // From the tray: a screenshot on the shelf, for a
                        // card. The board comes forward first, and hears of
                        // the picture a moment later, once it is there to
                        // hear.
                        if (command.startsWith("shot:")) {
                            focus_panel("board");
                            window.setTimeout(() => {
                                window.dispatchEvent(
                                    new CustomEvent("agentland:command", { detail: command }),
                                );
                            }, 400);
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

            const fresh = {
                mb_per_second: Number(((bytes * 2) / (1024 * 1024)).toFixed(2)),
                dropped_frames,
                collapsed_mb: Number((collapsed_bytes / (1024 * 1024)).toFixed(1)),
            };

            set_throughput((held) =>
                held.mb_per_second === fresh.mb_per_second &&
                held.dropped_frames === fresh.dropped_frames &&
                held.collapsed_mb === fresh.collapsed_mb
                    ? held
                    : fresh,
            );
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

    // Panes a person put away: the agent keeps running, the grid stops showing
    // it, and the sync below does not bring it back until somebody opens it.
    const put_away = useRef<Set<string>>(new Set());
    // The crew as last listed, readable from callbacks declared before it.
    const crew_ref = useRef<Agent[]>([]);

    const open_session = useCallback(async (session_id: string) => {
        put_away.current.delete(session_id);
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

    // Closing an agent's pane used to kill the agent: the commander's context
    // went with it, and its record still said "at its desk". A pane that
    // belongs to an agent is put away instead; stopping the agent is its own
    // action, on the crew panel and the pane's menu, where it says so.
    const close_session = useCallback(
        (id: string) => {
            set_sessions((held) => held.filter((entry) => entry.id !== id));
            metrics_ref.current.delete(id);

            if (crew_ref.current.some((agent) => agent.session_id === id)) {
                put_away.current.add(id);
                return;
            }

            kill_session(id).catch((cause) => set_error(String(cause)));
        },
        [],
    );

    const clear = useCallback(async () => {
        const current = await list_sessions();
        await Promise.all(current.map((session) => kill_session(session.id).catch(() => undefined)));
        metrics_ref.current.clear();
        run_ref.current = null;
        set_measuring(false);
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
            set_measuring(true);
            // A run writes lines for 30 s; the meter has no reason to keep
            // burning frames after that.
            window.setTimeout(() => set_measuring(false), 32_000);
            set_focused_id(created[0]?.id ?? null);
            set_sessions(created);
        } catch (cause) {
            set_error(String(cause));
        } finally {
            set_busy(false);
        }
    }, [clear, pane_count, rate, settings.duration_ms]);

    // Photographing the window belonged to the island panel, so it happened only
    // while that panel was open: the command was queued, drained, dispatched and
    // fell on nobody, silently, for hours. A picture of the window is not the
    // island's business.
    useEffect(() => {
        const listener = (event: Event) => {
            if ((event as CustomEvent<string>).detail !== "capture-window") {
                return;
            }

            void (async () => {
                try {
                    const { toPng } = await import("html-to-image");
                    const data = await toPng(document.body, { pixelRatio: 1 });

                    if (!is_tauri()) {
                        return;
                    }

                    const { invoke } = await import("@tauri-apps/api/core");
                    await invoke<string>("save_capture", { name: "window", data });
                } catch (cause) {
                    set_error(cause instanceof Error ? cause.message : String(cause));
                }
            })();
        };

        window.addEventListener("agentland:command", listener);
        return () => window.removeEventListener("agentland:command", listener);
    }, []);

    useEffect(() => {
        const listener = (event: Event) => {
            const command = (event as CustomEvent<string>).detail;
            if (!command.startsWith("bench:")) {
                return;
            }

            const wants_island = command === "bench:with-island";
            set_layout(bench_layout(wants_island));

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
                Array.from({ length: pane_count }, () => spawn_default_shell()),
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
    const [jumping, set_jumping] = useState(false);
    /// Bumped whenever something other than the tabs changes the active
    /// workspace, so the tabs go and read what is true rather than what they
    /// last set themselves.
    const [workspace_turn, set_workspace_turn] = useState(0);
    const [going, set_going] = useState<{ repository_id: string | null; worktree: string | null; at: number } | null>(
        null,
    );

    // The one key that reaches everywhere. Ctrl is what the header says, and a
    // Mac keyboard reaches the same box with Cmd.
    useEffect(() => {
        const listen = (event: KeyboardEvent) => {
            if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
                event.preventDefault();
                set_jumping((held) => !held);
            }
        };

        window.addEventListener("keydown", listen);
        return () => window.removeEventListener("keydown", listen);
    }, []);
    const [rail_shut, set_rail_shut] = useState(() => {
        try {
            return localStorage.getItem("agentland-rail") === "shut";
        } catch {
            return false;
        }
    });
    const [crew, set_crew] = useState<Agent[]>([]);
    useEffect(() => {
        crew_ref.current = crew;
    }, [crew]);

    // Where a notice, or a line on the tray, sends a person.
    const go_and_see = useCallback(
        (opens: string) => {
            const [what, which] = opens.split(":");
            if (what === "agent" && which) {
                // A notice about one agent is about what is on its screen — a
                // question it is holding, a limit it hit. The crew list says
                // that agent exists, which the notice had already said; the
                // pane is where the thing itself is.
                const held = crew.find((agent) => agent.id === which);
                if (held?.session_id) {
                    void open_session(held.session_id);
                    return;
                }

                // With no pane there is nothing to look at, and the list is
                // where somebody would start one.
                focus_panel("crew");
                return;
            }
            if (is_known_panel(what)) {
                focus_panel(what as PanelId);
            }
        },
        [crew, open_session, focus_panel],
    );
    go_and_see_ref.current = go_and_see;
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
    const verdict = !measuring
        ? "resting"
        : frame_stats.fps >= 55
          ? "pass"
          : frame_stats.fps >= 30
            ? "marginal"
            : "fail";

    const update_settings = useCallback((next: Settings) => {
        set_settings(next);
        save_settings(next);
    }, []);

    const open_window_menu = useCallback(
        (event: React.MouseEvent) => {
            menu.open(event, "Agentland", [
                { label: "Start a project", hint: "panel", run: () => focus_panel("start") },
                { label: "Island", hint: "panel", run: () => focus_panel("island") },
                { label: "Terminals", hint: "panel", run: () => focus_panel("panes") },
                { label: "Board", hint: "panel", run: () => focus_panel("board") },
                { label: "Repositories", hint: "panel", run: () => focus_panel("repos") },
                { label: "Crew", hint: "panel", run: () => focus_panel("crew") },
                { label: "Activity", hint: "panel", run: () => focus_panel("activity") },
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
                ...(update_out
                    ? [
                          {
                              label: `Update to ${update_out}`,
                              hint: "settings",
                              run: () => set_settings_open(true),
                          },
                      ]
                    : []),
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
        [menu, layout, focus_panel, update_out],
    );

    const services = useMemo<WorkspaceServices>(
        () => ({
            open_menu: menu.open,
            sessions: shown_sessions,
            crew,
            repositories: workspace_repos,
            going,
            open_session,
            close_session,
            open_shell_in: (cwd: string) => {
                spawn_default_shell(cwd)
                    .then((created) => {
                        set_sessions((held) => [...held, created]);
                        set_focused_id(created.id);
                    })
                    .catch((cause) => set_error(String(cause)));
            },
            focus_pane: set_focused_id,
            focused_id,
            on_metrics,
        }),
        [
            close_session,
            crew,
            focused_id,
            going,
            menu.open,
            on_metrics,
            open_session,
            shown_sessions,
            workspace_repos,
        ],
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
                    busy={busy}
                    on_run_benchmark={() => {
                        set_settings_open(false);
                        void run_benchmark();
                    }}
                    on_open_shells={() => {
                        set_settings_open(false);
                        void open_shells();
                    }}
                    on_clear={() => void clear()}
                />
            ) : null}

            <Jumper
                open={jumping}
                on_close={() => set_jumping(false)}
                on_go={(place) => {
                    set_workspace_turn((turn) => turn + 1);
                    set_going({
                        repository_id: place.repository_id,
                        worktree: place.worktree,
                        at: Date.now(),
                    });

                    if (place.kind === "agent") {
                        const held = crew.find((agent) => agent.id === place.agent_id);
                        if (held?.session_id) {
                            open_session(held.session_id);
                            focus_panel("panes");
                        } else {
                            focus_panel("crew");
                        }
                        return;
                    }

                    focus_panel(place.kind === "workspace" ? "island" : "project");
                }}
            />

            <header className="flex shrink-0 items-center gap-3 border-b border-reef/70 px-3 py-1.5">
                <WorkspaceTabs
                    turn={workspace_turn}
                    active={workspace_id}
                    on_active={(id, repositories) => {
                        set_workspace_id(id);
                        // The same list again is not a change; a fresh array
                        // for the same ids would render everything under it.
                        set_workspace_repos((held) =>
                            held === repositories ||
                            (held !== null &&
                                repositories !== null &&
                                held.length === repositories.length &&
                                held.every((entry, index) => entry === repositories[index]))
                                ? held
                                : repositories,
                        );
                    }}
                    on_switched={() => set_workspace_turn((turn) => turn + 1)}
                    counts={workspace_counts}
                />

                <span className="h-4 w-px bg-reef" />

                <PlaceTrail
                    repository_id={going?.repository_id ?? null}
                    worktree={going?.worktree ?? null}
                    turn={workspace_turn}
                    on_open={() => set_jumping(true)}
                />

                <span className="h-4 w-px bg-reef" />

                <div className="flex items-center gap-0.5 rounded-lg border border-reef p-0.5">
                    {PRESETS.map((preset) => {
                        const active = current_preset === preset.id;
                        return (
                            <button
                                key={preset.id}
                                title={preset.hint}
                                onClick={() => set_layout(preset.build())}
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
                        // Held down, a button is a button and not a paragraph:
                        // without this, holding it starts selecting the label.
                        className={`select-none rounded border px-2 py-[3px] font-mono text-[11px] transition-colors ${
                            !can_record
                                ? "border-reef text-shade"
                                : listening
                                  ? "animate-pulse border-coral bg-coral/15 text-coral"
                                  : reading
                                    ? "animate-pulse border-sun bg-sun/10 text-sun"
                                    : "border-reef text-shell hover:border-foam"
                        }`}
                        // A button that can only fail should say so before it is
                        // held, not after.
                        disabled={!can_record}
                        title={
                            can_record
                                ? "hold to speak — what you say is typed into the pane you are watching, not sent"
                                : "no recorder on this machine — Settings says what voice needs"
                        }
                        onPointerDown={() => {
                            set_heard(null);
                            begin_speaking()
                                .then(() => set_listening(true))
                                .catch((cause) => set_error(String(cause)));
                        }}
                        onPointerUp={() => {
                            if (!listening) {
                                return;
                            }

                            set_listening(false);
                            set_reading(true);
                            end_speaking()
                                .then((text) => {
                                    if (!text) {
                                        set_heard("nothing was said");
                                        return;
                                    }

                                    const pane = focused_id ?? shown_sessions[0]?.id;
                                    if (!pane) {
                                        set_heard(text);
                                        return;
                                    }

                                    set_heard(text);
                                    void write_input(pane, text);
                                })
                                .catch((cause) => set_error(String(cause)))
                                .finally(() => set_reading(false));
                        }}
                    >
                        {listening ? "● listening…" : reading ? "◐ reading it back…" : "◉ hold to speak"}
                    </button>
                </div>

                {heard ? (
                    <span className="max-w-[28rem] truncate font-mono text-[10px] text-turquoise" title={heard}>
                        “{heard}”
                    </span>
                ) : null}

                <div className="ml-auto flex items-center gap-3">
                    <NoticeBell on_open={go_and_see} />

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
                <div className="flex items-start gap-3 border-b border-coral bg-lagoon px-4 py-2 font-mono text-xs text-coral">
                    <span className="min-w-0 flex-1 break-words">{error}</span>
                    <button
                        className="shrink-0 rounded border border-coral/50 px-1.5 leading-5 hover:bg-coral/10"
                        title="dismiss"
                        aria-label="dismiss this error"
                        onClick={() => set_error(null)}
                    >
                        ×
                    </button>
                </div>
            ) : null}

            <div className="flex min-h-0 min-w-0 flex-1">
                <WorkspaceRail
                    visible={visible}
                    repositories={workspace_repos}
                    active_workspace={workspace_id}
                    on_switched={() => set_workspace_turn((turn) => turn + 1)}
                    on_open_repo={(repo) => {
                        set_going({ repository_id: repo.id, worktree: null, at: Date.now() });
                        focus_panel("project");
                    }}
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
                            <span className={verdict === "resting" ? "text-shade" : verdict === "pass" ? "text-palm" : verdict === "marginal" ? "text-sun" : "text-coral"}>
                                {frame_stats.fps} fps
                            </span>
                        </div>
                    }
                />

            <ServiceProvider services={services}>
                <Workspace
                    layout={layout}
                    on_layout={set_layout}
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
            </ServiceProvider>
            </div>

            <footer className="flex shrink-0 items-center justify-end gap-3 border-t border-reef/70 px-3 py-[3px] font-mono text-[10px] tabular-nums">
                <span
                    className={
                        verdict === "resting"
                            ? "text-shade"
                            : verdict === "pass"
                              ? "text-palm"
                              : verdict === "marginal"
                                ? "text-sun"
                                : "text-coral"
                    }
                    title={measuring ? "frames the app draws" : "frames the island draws — the app measures itself only while benchmarking"}
                >
                    {frame_stats.fps} fps{measuring ? "" : " · island"}
                </span>
                <span className="text-shade">worst {frame_stats.worst_frame_ms} ms</span>
                <span className="text-shade">{throughput.mb_per_second} MB/s</span>
                <span className="text-shade">
                    drop {throughput.dropped_frames} · collapsed {throughput.collapsed_mb} MB
                </span>
                <span className="text-shade" title={gpu.renderer}>
                    {gpu.webgl2 ? "webgl2" : gpu.renderer === "none" ? "none" : "webgl1"} · {gpu.max_contexts} ctx
                </span>
            </footer>
        </div>
    );
}
