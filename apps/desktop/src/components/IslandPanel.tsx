import { useCallback, useEffect, useRef, useState } from "react";
import * as THREE from "three";

import { AgentSheet } from "@/components/AgentSheet";
import { Island } from "@/island/Island";
import { PRESENCE_COLOR, tier_for } from "@/island/geometry";
import {
    assign_task,
    dispatch_status,
    dispatch_task,
    list_agents,
    list_tasks,
    pause_dispatch,
    type Agent,
    type DispatchState,
    type Task,
} from "@/lib/core";
import { probe_gpu } from "@/lib/gpu";
import { is_tauri } from "@/lib/core";

interface Props {
    active: boolean;
    on_open_session: (session_id: string) => void;
}

export function IslandPanel({ active, on_open_session }: Props) {
    const [agents, set_agents] = useState<Agent[]>([]);
    const [tasks, set_tasks] = useState<Task[]>([]);
    const [hovered, set_hovered] = useState<string | null>(null);
    const [message, set_message] = useState<string | null>(null);
    const [webgl] = useState(() => probe_gpu(1).renderer !== "none");
    const [dispatch, set_dispatch] = useState<DispatchState | null>(null);
    const [shots, set_shots] = useState<Array<{ seq: number; agent_id: string }>>([]);
    const [selected, set_selected] = useState<string | null>(null);
    const label_layer = useRef<HTMLDivElement>(null);
    const drag_origin = useRef<{ x: number; y: number } | null>(null);
    const seen_seq = useRef(0);
    const container_ref = useRef<HTMLDivElement>(null);
    const scene_ref = useRef<{ scene: THREE.Scene; camera: THREE.Camera } | null>(null);
    const invalidate_ref = useRef<(() => void) | null>(null);
    const raycaster = useRef(new THREE.Raycaster());

    const refresh = useCallback(async () => {
        const [crew, board, manager] = await Promise.all([
            list_agents(),
            list_tasks(),
            dispatch_status(),
        ]);
        set_agents(crew);
        set_tasks(board.filter((task) => !task.assignee));
        set_dispatch(manager);

        const fresh = (manager.events ?? []).filter((event) => event.seq > seen_seq.current);
        if (fresh.length > 0) {
            seen_seq.current = Math.max(...fresh.map((event) => event.seq));
            set_shots((current) => [
                ...current,
                ...fresh.map((event) => ({ seq: event.seq, agent_id: event.agent_id })),
            ]);
        }
    }, []);

    useEffect(() => {
        refresh().catch((cause) => set_message(String(cause)));
        const handle = window.setInterval(
            () => {
                refresh().catch(() => undefined);
            },
            active ? 4000 : 15000,
        );
        return () => window.clearInterval(handle);
    }, [refresh, active]);

    const agent_at = useCallback((event: React.DragEvent<HTMLDivElement>) => {
        const container = container_ref.current;
        const context = scene_ref.current;
        if (!container || !context) {
            return null;
        }

        const bounds = container.getBoundingClientRect();
        const pointer = new THREE.Vector2(
            ((event.clientX - bounds.left) / bounds.width) * 2 - 1,
            -((event.clientY - bounds.top) / bounds.height) * 2 + 1,
        );

        raycaster.current.setFromCamera(pointer, context.camera);
        const hits = raycaster.current.intersectObjects(context.scene.children, true);

        for (const hit of hits) {
            let node: THREE.Object3D | null = hit.object;
            while (node) {
                const id = node.userData?.agent_id;
                if (typeof id === "string") {
                    return id;
                }
                if (node.userData?.dispatch === true) {
                    return "__dispatch__";
                }
                node = node.parent;
            }
        }

        return nearest_marker(pointer, bounds, context.camera);
    }, []);

    const nearest_marker = useCallback(
        (pointer: THREE.Vector2, bounds: DOMRect, camera: THREE.Camera): string | null => {
            const context = scene_ref.current;
            if (!context) {
                return null;
            }

            const projected = new THREE.Vector3();
            let best_id: string | null = null;
            let best_distance = Number.POSITIVE_INFINITY;

            context.scene.traverse((node) => {
                const id = node.userData?.agent_id;
                const dispatch = node.userData?.dispatch === true;
                if (typeof id !== "string" && !dispatch) {
                    return;
                }

                node.getWorldPosition(projected);
                projected.project(camera);

                const distance = Math.hypot(projected.x - pointer.x, projected.y - pointer.y);
                const key = typeof id === "string" ? id : "__dispatch__";

                if (distance < best_distance) {
                    best_distance = distance;
                    best_id = key;
                }
            });

            const tolerance = (90 / Math.max(bounds.width, bounds.height)) * 2;
            return best_distance < tolerance ? best_id : null;
        },
        [],
    );

    const capture = useCallback(async (whole_window = false) => {
        let data: string;

        if (whole_window) {
            invalidate_ref.current?.();
            await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));

            const { toPng } = await import("html-to-image");
            data = await toPng(document.getElementById("root") as HTMLElement, {
                pixelRatio: 1,
                backgroundColor: "#0d1c1f",
            });
        } else {
            const canvas = container_ref.current?.querySelector("canvas");
            if (!canvas) {
                set_message("No island to capture.");
                return;
            }
            data = canvas.toDataURL("image/png");
        }

        if (!is_tauri()) {
            const anchor = document.createElement("a");
            anchor.href = data;
            anchor.download = "island.png";
            anchor.click();
            return;
        }

        try {
            const { invoke } = await import("@tauri-apps/api/core");
            const path = await invoke<string>("save_capture", {
                name: whole_window ? "window" : "island",
                data,
            });
            set_message(`Saved ${path}`);
        } catch (cause) {
            set_message(cause instanceof Error ? cause.message : String(cause));
        }
    }, []);

    useEffect(() => {
        const shortcut = () => void capture(false);
        window.addEventListener("agentland:capture-island", shortcut);

        const commands = (event: Event) => {
            const command = (event as CustomEvent<string>).detail;

            if (command === "capture-island") {
                void capture(false);
            } else if (command === "capture-window") {
                void capture(true);
            } else if (command.startsWith("select-agent:")) {
                set_selected(command.slice("select-agent:".length));
            }
        };

        window.addEventListener("agentland:command", commands);

        return () => {
            window.removeEventListener("agentland:capture-island", shortcut);
            window.removeEventListener("agentland:command", commands);
        };
    }, [capture, agent_at]);

    const place_labels = useCallback(
        (marks: Array<{ id: string; x: number; y: number; visible: boolean }>) => {
            const layer = label_layer.current;
            if (!layer) {
                return;
            }

            for (const mark of marks) {
                const node = layer.querySelector<HTMLElement>(`[data-agent="${mark.id}"]`);
                if (!node) {
                    continue;
                }

                node.style.transform = `translate3d(${Math.round(mark.x)}px, ${Math.round(mark.y)}px, 0) translate(-50%, -100%)`;
                node.style.visibility = mark.visible ? "visible" : "hidden";
            }
        },
        [],
    );

    const tier = tier_for(agents.length);
    const selected_agent = agents.find((agent) => agent.id === selected) ?? null;

    return (
        <div className="flex h-full min-h-0 min-w-0 flex-1">
            <aside className="flex w-72 shrink-0 flex-col border-r border-reef">
                <header className="border-b border-reef px-3 py-2 font-mono text-[11px] uppercase tracking-[0.1em] text-shell">
                    Unassigned · drag onto a station
                </header>

                <div className="flex h-full min-h-0 min-w-0 flex-1 flex-col gap-2 overflow-y-auto p-2">
                    {tasks.length === 0 ? (
                        <p className="font-mono text-[11px] text-shade">
                            Nothing waiting. Cards created on the board appear here until they have an
                            owner.
                        </p>
                    ) : null}

                    {tasks.map((task) => (
                        <article
                            key={task.id}
                            draggable
                            onDragStart={(event) => event.dataTransfer.setData("text/plain", task.id)}
                            className="cursor-grab border border-reef bg-lagoon p-2 rounded-lg"
                        >
                            <div className="text-xs text-linen">{task.title}</div>
                            <div className="mt-1 font-mono text-[10px] text-shade">
                                {task.id} · {task.repository_id}
                            </div>
                        </article>
                    ))}
                </div>

                <footer className="flex flex-col gap-2 border-t border-reef px-3 py-2 font-mono text-[10px] text-shade">
                    <span>
                        {agents.length} crew · {tier.label}
                    </span>

                    {dispatch ? (
                        <div className="flex items-center justify-between gap-2">
                            <span className={dispatch.paused ? "text-sun" : "text-palm"}>
                                X {dispatch.paused ? "paused" : "on duty"}
                                {dispatch.queue.length > 0 ? ` · ${dispatch.queue.length} queued` : ""}
                            </span>
                            <button
                                className="border border-reef px-2 py-1 rounded-lg"
                                onClick={() =>
                                    pause_dispatch(!dispatch.paused)
                                        .then(set_dispatch)
                                        .catch((cause) => set_message(String(cause)))
                                }
                            >
                                {dispatch.paused ? "resume" : "pause"}
                            </button>
                        </div>
                    ) : null}
                </footer>
            </aside>

            <div
                ref={container_ref}
                onPointerDown={(event) => {
                    drag_origin.current = { x: event.clientX, y: event.clientY };
                }}
                onClick={(event) => {
                    const origin = drag_origin.current;
                    drag_origin.current = null;

                    if (origin && Math.hypot(event.clientX - origin.x, event.clientY - origin.y) > 6) {
                        return;
                    }

                    const hit = agent_at(event as unknown as React.DragEvent<HTMLDivElement>);
                    if (hit && hit !== "__dispatch__") {
                        set_selected(hit);
                    } else if (hit === "__dispatch__") {
                        set_message("That is X's lighthouse — drop a card on it to hand work over.");
                    }
                }}
                className="relative min-h-0 min-w-0 flex-1"
                onDragOver={(event) => {
                    event.preventDefault();
                    set_hovered(agent_at(event));
                }}
                onDragLeave={() => set_hovered(null)}
                onDrop={(event) => {
                    event.preventDefault();
                    const task_id = event.dataTransfer.getData("text/plain");
                    const agent_id = agent_at(event);
                    set_hovered(null);

                    if (!task_id || !agent_id) {
                        set_message("Drop a card on a station to assign it.");
                        return;
                    }

                    const action =
                        agent_id === "__dispatch__"
                            ? dispatch_task(task_id).then((report) => {
                                  const { outcome, reason } = report.decision;
                                  set_message(`X ${outcome}: ${reason}`);
                              })
                            : assign_task(task_id, agent_id).then(() => {
                                  set_message(`${task_id} assigned to ${agent_id}`);
                              });

                    action
                        .then(() => refresh())
                        .catch((cause) =>
                            set_message(cause instanceof Error ? cause.message : String(cause)),
                        );
                }}
            >
                {webgl ? (
                    <Island
                        agents={agents}
                        seed={agents.map((agent) => agent.id).join("-") || "empty"}
                        active={active}
                        highlighted={hovered}
                        paused={dispatch?.paused ?? false}
                        shots={shots}
                        on_project={place_labels}
                        selected={selected}
                        on_select={(id) => set_selected(id)}
                        on_shot_done={(seq) =>
                            set_shots((current) => current.filter((shot) => shot.seq !== seq))
                        }
                        on_scene={(scene, camera, invalidate) => {
                            scene_ref.current = { scene, camera };
                            invalidate_ref.current = invalidate;
                        }}
                    />
                ) : (
                    <div className="flex h-full flex-col items-center justify-center gap-3 p-6">
                        <p className="max-w-sm text-center font-mono text-xs text-sun">
                            This webview grants no WebGL context, so the island falls back to a list. The
                            same states are shown.
                        </p>
                        <div className="flex flex-wrap justify-center gap-2">
                            {agents.map((agent) => (
                                <span
                                    key={agent.id}
                                    className="border border-reef px-2 py-1 font-mono text-[11px] rounded-lg"
                                >
                                    {agent.name} · {agent.role} · {agent.presence}
                                </span>
                            ))}
                        </div>
                    </div>
                )}

                <div ref={label_layer} className="pointer-events-none absolute inset-0">
                    {webgl
                        ? agents.map((agent) => (
                              <div
                                  key={agent.id}
                                  data-agent={agent.id}
                                  className="absolute left-0 top-0 whitespace-nowrap rounded-full border border-reef px-2 py-[2px] font-mono text-[10px] text-linen"
                                  style={{
                                      visibility: "hidden",
                                      willChange: "transform",
                                      backgroundColor: "rgba(13, 28, 31, 0.92)",
                                  }}
                              >
                                  <span
                                      className="mr-1 inline-block h-[6px] w-[6px] rounded-full align-middle"
                                      style={{
                                          backgroundColor:
                                              PRESENCE_COLOR[agent.presence] ?? PRESENCE_COLOR.idle,
                                      }}
                                  />
                                  {agent.name}
                              </div>
                          ))
                        : null}
                </div>

                {message ? (
                    <div className="absolute bottom-3 left-3 border border-reef bg-lagoon px-3 py-2 font-mono text-[11px] text-driftwood rounded-lg">
                        {message}
                    </div>
                ) : null}

                {hovered ? (
                    <div className="absolute right-3 top-3 border border-turquoise bg-shallow px-3 py-2 font-mono text-[11px] text-turquoise rounded-lg">
                        {hovered === "__dispatch__" ? "drop to hand to X" : `drop to assign → ${hovered}`}
                    </div>
                ) : null}

                {selected_agent ? (
                    <AgentSheet
                        agent={selected_agent}
                        on_close={() => set_selected(null)}
                        on_open_pane={(session_id) => {
                            set_selected(null);
                            on_open_session(session_id);
                        }}
                        on_changed={() => void refresh()}
                    />
                ) : null}
            </div>
        </div>
    );
}
