import { useCallback, useEffect, useRef, useState } from "react";
import * as THREE from "three";

import { Island } from "@/island/Island";
import { tier_for } from "@/island/geometry";
import { assign_task, list_agents, list_tasks, type Agent, type Task } from "@/lib/core";
import { probe_gpu } from "@/lib/gpu";

interface Props {
    active: boolean;
}

export function IslandPanel({ active }: Props) {
    const [agents, set_agents] = useState<Agent[]>([]);
    const [tasks, set_tasks] = useState<Task[]>([]);
    const [hovered, set_hovered] = useState<string | null>(null);
    const [message, set_message] = useState<string | null>(null);
    const [webgl] = useState(() => probe_gpu(1).renderer !== "none");
    const container_ref = useRef<HTMLDivElement>(null);
    const scene_ref = useRef<{ scene: THREE.Scene; camera: THREE.Camera } | null>(null);
    const raycaster = useRef(new THREE.Raycaster());

    const refresh = useCallback(async () => {
        const [crew, board] = await Promise.all([list_agents(), list_tasks()]);
        set_agents(crew);
        set_tasks(board.filter((task) => !task.assignee));
    }, []);

    useEffect(() => {
        refresh().catch((cause) => set_message(String(cause)));
        const handle = window.setInterval(() => {
            refresh().catch(() => undefined);
        }, 4000);
        return () => window.clearInterval(handle);
    }, [refresh]);

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
                node = node.parent;
            }
        }

        return null;
    }, []);

    const tier = tier_for(agents.length);

    return (
        <div className="flex min-h-0 flex-1">
            <aside className="flex w-72 shrink-0 flex-col border-r border-[#26343a]">
                <header className="border-b border-[#26343a] px-3 py-2 font-mono text-[11px] uppercase tracking-[0.1em] text-[#7b8d94]">
                    Unassigned · drag onto a station
                </header>

                <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto p-2">
                    {tasks.length === 0 ? (
                        <p className="font-mono text-[11px] text-[#5d6e75]">
                            Nothing waiting. Cards created on the board appear here until they have an
                            owner.
                        </p>
                    ) : null}

                    {tasks.map((task) => (
                        <article
                            key={task.id}
                            draggable
                            onDragStart={(event) => event.dataTransfer.setData("text/plain", task.id)}
                            className="cursor-grab border border-[#26343a] bg-[#141c1f] p-2"
                        >
                            <div className="text-xs text-[#e3ebee]">{task.title}</div>
                            <div className="mt-1 font-mono text-[10px] text-[#5d6e75]">
                                {task.id} · {task.repository_id}
                            </div>
                        </article>
                    ))}
                </div>

                <footer className="border-t border-[#26343a] px-3 py-2 font-mono text-[10px] text-[#5d6e75]">
                    {agents.length} crew · {tier.label}
                </footer>
            </aside>

            <div
                ref={container_ref}
                className="relative min-h-0 flex-1"
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

                    assign_task(task_id, agent_id)
                        .then(() => {
                            set_message(`${task_id} assigned to ${agent_id}`);
                            return refresh();
                        })
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
                        on_scene={(scene, camera) => {
                            scene_ref.current = { scene, camera };
                        }}
                    />
                ) : (
                    <div className="flex h-full flex-col items-center justify-center gap-3 p-6">
                        <p className="max-w-sm text-center font-mono text-xs text-[#c99a2e]">
                            This webview grants no WebGL context, so the island falls back to a list. The
                            same states are shown.
                        </p>
                        <div className="flex flex-wrap justify-center gap-2">
                            {agents.map((agent) => (
                                <span
                                    key={agent.id}
                                    className="border border-[#26343a] px-2 py-1 font-mono text-[11px]"
                                >
                                    {agent.name} · {agent.role} · {agent.state}
                                </span>
                            ))}
                        </div>
                    </div>
                )}

                {message ? (
                    <div className="absolute bottom-3 left-3 border border-[#26343a] bg-[#141c1f] px-3 py-2 font-mono text-[11px] text-[#a4b5bb]">
                        {message}
                    </div>
                ) : null}

                {hovered ? (
                    <div className="absolute right-3 top-3 border border-[#45bcc4] bg-[#14343a] px-3 py-2 font-mono text-[11px] text-[#45bcc4]">
                        drop to assign → {hovered}
                    </div>
                ) : null}
            </div>
        </div>
    );
}
