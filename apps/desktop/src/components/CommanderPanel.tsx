import { useCallback, useEffect, useState } from "react";

import {
    list_plans,
    mark_step,
    ready_steps,
    supervisor_watches,
    type Plan,
    type PlanStep,
    type ReadyStep,
    type Watch,
} from "@/lib/core";
import { use_services } from "@/workspace/registry";

const STEP_COLOR: Record<string, string> = {
    waiting: "text-shade",
    assigned: "text-sun",
    done: "text-palm",
    blocked: "text-coral",
};

export function CommanderPanel({ active }: { active: boolean }) {
    const { crew, open_session } = use_services();
    const [plans, set_plans] = useState<Plan[]>([]);
    const [ready, set_ready] = useState<ReadyStep[]>([]);
    const [watches, set_watches] = useState<Watch[]>([]);
    const [notice, set_notice] = useState<string | null>(null);

    const refresh = useCallback(async () => {
        const [held, next, watching] = await Promise.all([
            list_plans(),
            ready_steps(),
            supervisor_watches(),
        ]);
        set_plans(held);
        set_ready(next);
        set_watches(watching);
    }, []);

    useEffect(() => {
        if (!active) {
            return;
        }

        refresh().catch((cause) => set_notice(cause instanceof Error ? cause.message : String(cause)));
        const handle = window.setInterval(() => refresh().catch(() => undefined), 4000);
        return () => window.clearInterval(handle);
    }, [active, refresh]);

    const commander = crew.find((agent) => agent.role === "commander");
    const running = plans.filter((plan) => plan.state === "running");
    const watching = watches.filter((watch) => watch.state === "working");
    const settled = watches.filter((watch) => watch.state !== "working");

    const step_of = (watch: Watch): PlanStep | undefined =>
        plans.find((plan) => plan.id === watch.plan_id)?.steps.find((step) => step.id === watch.step_id);

    return (
        <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-2 overflow-y-auto p-2.5">
            <section className="flex flex-wrap items-center gap-2">
                {commander ? (
                    <button
                        className="rounded-md border border-turquoise px-2 py-0.5 font-mono text-[11px] text-turquoise"
                        onClick={() => commander.session_id && open_session(commander.session_id)}
                    >
                        {commander.name} · {commander.session_id ? "at its desk" : "not started"}
                    </button>
                ) : (
                    <span className="font-mono text-[11px] text-shade">
                        No commander hired. Hire an agent with the role commander.
                    </span>
                )}
                <span className="font-mono text-[10px] text-shade">
                    {running.length} plan{running.length === 1 ? "" : "s"} running · {watching.length} step
                    {watching.length === 1 ? "" : "s"} being watched
                </span>
            </section>

            {notice ? (
                <div className="rounded-md border border-coral px-2 py-1 font-mono text-[11px] text-coral">
                    {notice}
                </div>
            ) : null}

            {ready.length > 0 ? (
                <section>
                    <h3 className="mb-1 font-mono text-[9px] uppercase tracking-[0.14em] text-turquoise">
                        Ready to start · {ready.length}
                    </h3>
                    <div className="flex flex-col gap-1">
                        {ready.map((entry) => (
                            <article
                                key={`${entry.plan_id}-${entry.step.id}`}
                                className="rounded-md border border-turquoise/50 bg-lagoon-deep px-2 py-1"
                            >
                                <div className="text-[12px] text-linen">{entry.step.title}</div>
                                <div className="font-mono text-[10px] text-shade">
                                    {entry.step.id} · {entry.repository_id} · nothing is waiting on it
                                </div>
                            </article>
                        ))}
                    </div>
                </section>
            ) : null}

            <section>
                <h3 className="mb-1 font-mono text-[9px] uppercase tracking-[0.14em] text-shade">
                    Plans · {plans.length}
                </h3>

                {plans.length === 0 ? (
                    <p className="font-mono text-[10px] text-shade">
                        No plan yet. Give the commander a goal and it takes it apart into steps.
                    </p>
                ) : null}

                <div className="flex flex-col gap-1.5">
                    {plans.map((plan) => {
                        const done = plan.steps.filter((step) => step.state === "done").length;

                        return (
                            <article
                                key={plan.id}
                                className="rounded-md border border-reef bg-lagoon-deep px-2 py-1.5"
                            >
                                <div className="flex items-baseline gap-2">
                                    <span className="text-[12px] text-linen">{plan.goal}</span>
                                    <span
                                        className={`ml-auto font-mono text-[10px] ${
                                            plan.state === "done" ? "text-palm" : "text-shade"
                                        }`}
                                    >
                                        {done}/{plan.steps.length} · {plan.state}
                                    </span>
                                </div>

                                <div className="mt-1 flex flex-col gap-0.5">
                                    {plan.steps.map((step) => (
                                        <div key={step.id} className="flex items-baseline gap-2">
                                            <span
                                                className={`font-mono text-[10px] ${
                                                    STEP_COLOR[step.state] ?? "text-shade"
                                                }`}
                                            >
                                                {step.state.padEnd(8)}
                                            </span>
                                            <span className="truncate text-[11px] text-driftwood">
                                                {step.title}
                                            </span>
                                            {step.needs.length > 0 ? (
                                                <span className="shrink-0 font-mono text-[10px] text-shade">
                                                    waits for {step.needs.join(", ")}
                                                </span>
                                            ) : null}
                                            {step.state === "assigned" ? (
                                                <button
                                                    className="ml-auto shrink-0 rounded border border-palm px-1.5 font-mono text-[10px] text-palm"
                                                    title="mark it done after reading the evidence"
                                                    onClick={() =>
                                                        mark_step(plan.id, step.id, "done")
                                                            .then(() => refresh())
                                                            .catch((cause) =>
                                                                set_notice(String(cause)),
                                                            )
                                                    }
                                                >
                                                    done
                                                </button>
                                            ) : null}
                                        </div>
                                    ))}
                                </div>
                            </article>
                        );
                    })}
                </div>
            </section>

            <section className="min-h-0">
                <h3 className="mb-1 font-mono text-[9px] uppercase tracking-[0.14em] text-shade">
                    What the supervisor is watching
                </h3>

                {watches.length === 0 ? (
                    <p className="font-mono text-[10px] text-shade">
                        Nothing delegated yet. A step given to an agent is followed until it settles.
                    </p>
                ) : null}

                <div className="flex flex-col gap-1">
                    {[...watching, ...settled].map((watch) => (
                        <article
                            key={watch.id}
                            className={`rounded-md border bg-lagoon-deep px-2 py-1 ${
                                watch.state === "working" ? "border-sun/60" : "border-reef"
                            }`}
                        >
                            <div className="flex flex-wrap items-baseline gap-2 font-mono text-[10px]">
                                <span className="text-linen">{watch.agent_id}</span>
                                <span className="text-shade">{step_of(watch)?.title ?? watch.step_id}</span>
                                <span
                                    className={
                                        watch.state === "working"
                                            ? "text-sun"
                                            : watch.state === "settled"
                                              ? "text-palm"
                                              : "text-coral"
                                    }
                                >
                                    {watch.state}
                                </span>
                                <span className="ml-auto text-shade">
                                    {watch.delivered ? "brief landed" : "brief not seen yet"}
                                    {watch.resends > 0 ? ` · resent ${watch.resends}×` : ""}
                                    {watch.reaped ? " · pane reclaimed" : ""}
                                </span>
                            </div>
                            {watch.reason ? (
                                <div className="mt-0.5 text-[11px] text-driftwood">{watch.reason}</div>
                            ) : null}
                        </article>
                    ))}
                </div>
            </section>
        </div>
    );
}
