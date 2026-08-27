import { useCallback, useEffect, useState } from "react";

import {
    dispatch_status,
    dispatch_task,
    list_tasks,
    pause_dispatch,
    set_dispatch_caps,
    type DispatchDecision,
    type DispatchState,
    type Task,
} from "@/lib/core";
import { use_services } from "@/workspace/registry";

export function DispatchPanel({ active }: { active: boolean }) {
    const { crew, repositories } = use_services();
    const [state, set_state] = useState<DispatchState | null>(null);
    const [tasks, set_tasks] = useState<Task[]>([]);
    const [last, set_last] = useState<{ task: string; decision: DispatchDecision } | null>(null);
    const [notice, set_notice] = useState<string | null>(null);

    const refresh = useCallback(async () => {
        const [held, board] = await Promise.all([dispatch_status(), list_tasks()]);
        set_state(held);
        set_tasks(board);
    }, []);

    useEffect(() => {
        if (!active) {
            return;
        }

        refresh().catch((cause) => set_notice(cause instanceof Error ? cause.message : String(cause)));
        const handle = window.setInterval(() => refresh().catch(() => undefined), 4000);
        return () => window.clearInterval(handle);
    }, [active, refresh]);

    const run = useCallback(
        (action: () => Promise<unknown>) => {
            set_notice(null);
            action()
                .then(() => refresh())
                .catch((cause) => set_notice(cause instanceof Error ? cause.message : String(cause)));
        },
        [refresh],
    );

    const unassigned = tasks.filter(
        (task) =>
            !task.assignee &&
            task.column !== "done" &&
            (!repositories || repositories.includes(task.repository_id)),
    );

    const hand_over = useCallback(
        (task: Task) => {
            set_notice(null);
            dispatch_task(task.id)
                .then((report) => {
                    set_last({ task: task.id, decision: report.decision });
                    set_state(report.state);
                    return refresh();
                })
                .catch((cause) => set_notice(cause instanceof Error ? cause.message : String(cause)));
        },
        [refresh],
    );

    const name_of = (id: string) => crew.find((agent) => agent.id === id)?.name ?? id;

    return (
        <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-2 overflow-y-auto p-2.5">
            <section className="flex flex-wrap items-center gap-1.5">
                <span
                    className={`rounded-md border px-2 py-0.5 font-mono text-[11px] ${
                        state?.paused ? "border-sun text-sun" : "border-palm text-palm"
                    }`}
                >
                    {state?.paused ? "X is holding everything" : "X is on duty"}
                </span>
                <button
                    className="rounded-md border border-foam px-2 py-0.5 font-mono text-[11px]"
                    disabled={!state}
                    onClick={() => state && run(() => pause_dispatch(!state.paused))}
                >
                    {state?.paused ? "let X hand out work" : "hold everything"}
                </button>

                <label className="flex items-center gap-1 font-mono text-[11px] text-shell">
                    per repository
                    <input
                        type="number"
                        min={1}
                        className="w-12 rounded-md border border-reef bg-lagoon-deep font-mono text-[11px]"
                        value={state?.caps.per_repository ?? 3}
                        onChange={(event) =>
                            state &&
                            run(() =>
                                set_dispatch_caps({
                                    ...state.caps,
                                    per_repository: Number(event.target.value) || 1,
                                }),
                            )
                        }
                    />
                </label>
                <label className="flex items-center gap-1 font-mono text-[11px] text-shell">
                    per engine
                    <input
                        type="number"
                        min={1}
                        className="w-12 rounded-md border border-reef bg-lagoon-deep font-mono text-[11px]"
                        value={state?.caps.per_engine ?? 2}
                        onChange={(event) =>
                            state &&
                            run(() =>
                                set_dispatch_caps({
                                    ...state.caps,
                                    per_engine: Number(event.target.value) || 1,
                                }),
                            )
                        }
                    />
                </label>

                {state && state.queue.length > 0 ? (
                    <span className="font-mono text-[10px] text-sun">
                        {state.queue.length} queued: {state.queue.join(", ")}
                    </span>
                ) : null}
            </section>

            {notice ? (
                <div className="rounded-md border border-coral px-2 py-1 font-mono text-[11px] text-coral">
                    {notice}
                </div>
            ) : null}

            {last ? (
                <div
                    className={`rounded-md border px-2 py-1 font-mono text-[11px] ${
                        last.decision.outcome === "assign"
                            ? "border-palm text-palm"
                            : last.decision.outcome === "queue"
                              ? "border-sun text-sun"
                              : "border-coral text-coral"
                    }`}
                >
                    {last.task} → {last.decision.outcome}
                    {last.decision.outcome === "assign" ? ` to ${name_of(last.decision.agent_id)}` : ""} ·{" "}
                    {last.decision.reason}
                </div>
            ) : null}

            <section>
                <h3 className="mb-1 font-mono text-[9px] uppercase tracking-[0.14em] text-shade">
                    Waiting for an owner · {unassigned.length}
                </h3>
                {unassigned.length === 0 ? (
                    <p className="font-mono text-[10px] text-shade">
                        Every card has an owner. X only decides when one does not.
                    </p>
                ) : null}
                <div className="flex flex-col gap-1">
                    {unassigned.map((task) => (
                        <article
                            key={task.id}
                            className="flex items-center gap-2 rounded-md border border-reef bg-lagoon-deep px-2 py-1"
                        >
                            <span className="truncate text-[12px] text-linen">{task.title}</span>
                            <span className="font-mono text-[10px] text-shade">
                                {task.id} · {task.repository_id}
                            </span>
                            <button
                                className="ml-auto shrink-0 rounded border border-turquoise px-1.5 font-mono text-[10px] text-turquoise"
                                onClick={() => hand_over(task)}
                            >
                                ask X
                            </button>
                        </article>
                    ))}
                </div>
            </section>

            <section className="min-h-0">
                <h3 className="mb-1 font-mono text-[9px] uppercase tracking-[0.14em] text-shade">
                    What X decided
                </h3>
                {!state || state.events.length === 0 ? (
                    <p className="font-mono text-[10px] text-shade">
                        Nothing handed out yet. Each decision is kept with the reason it was made.
                    </p>
                ) : null}
                <div className="flex flex-col gap-1">
                    {state
                        ? [...state.events].reverse().map((event) => (
                              <article
                                  key={event.seq}
                                  className="rounded-md border border-reef bg-lagoon-deep px-2 py-1"
                              >
                                  <div className="flex items-baseline gap-2 font-mono text-[10px]">
                                      <span className="text-shade">#{event.seq}</span>
                                      <span className="text-linen">{name_of(event.agent_id)}</span>
                                      <span className="text-shade">took</span>
                                      <span className="text-linen">{event.task_id}</span>
                                  </div>
                                  <div className="mt-0.5 text-[11px] text-driftwood">{event.reason}</div>
                              </article>
                          ))
                        : null}
                </div>
            </section>
        </div>
    );
}
