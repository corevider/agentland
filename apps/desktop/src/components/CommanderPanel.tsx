import { use_poll } from "@/lib/poll";
import { useCallback, useEffect, useState } from "react";

import {
    clear_goal,
    read_goals,
    set_goal,
    ignite_commander,
    list_plans,
    list_repos,
    mark_step,
    ready_steps,
    supervisor_watches,
    type Plan,
    type PlanStep,
    type ReadyStep,
    type Goal,
    type Repository,
    type Watch,
} from "@/lib/core";
import { Waiting } from "@/components/Spinner";
import { use_services } from "@/workspace/registry";

const STEP_COLOR: Record<string, string> = {
    waiting: "text-shade",
    assigned: "text-sun",
    done: "text-palm",
    blocked: "text-coral",
};

export function CommanderPanel({ active }: { active: boolean }) {
    const { crew, repositories, open_session } = use_services();
    const [plans, set_plans] = useState<Plan[]>([]);
    const [ready, set_ready] = useState<ReadyStep[]>([]);
    const [watches, set_watches] = useState<Watch[]>([]);
    const [repos, set_repos] = useState<Repository[]>([]);
    const [igniting, set_igniting] = useState<string | null>(null);
    const [notice, set_notice] = useState<string | null>(null);
    const [goals, set_goals] = useState<Goal[]>([]);
    const [writing, set_writing] = useState<string | null>(null);
    const [draft, set_draft] = useState("");

    const refresh = useCallback(async () => {
        const [held, next, watching, known, wanted] = await Promise.all([
            list_plans(),
            ready_steps(),
            supervisor_watches(),
            list_repos(),
            read_goals(),
        ]);
        set_plans(held);
        set_ready(next);
        set_watches(watching);
        set_repos(known);
        set_goals(wanted);
    }, []);

    use_poll(() => {
        refresh().catch(() => undefined);
    }, 4000, active);

    // A commander belongs to a project. Reading the first one in the crew showed
    // one project's X while standing in another's, which is the sort of wrong
    // that looks right.
    const mine = repositories ? repos.filter((repo) => repositories.includes(repo.id)) : repos;
    const commander_of = (repository_id: string) =>
        crew.find((agent) => agent.role === "commander" && agent.repository_id === repository_id);

    const save_goal = useCallback(
        async (repository_id: string) => {
            try {
                await set_goal(repository_id, draft);
                set_writing(null);
                set_notice(null);
                await refresh();
            } catch (cause) {
                set_notice(cause instanceof Error ? cause.message : String(cause));
            }
        },
        [draft, refresh],
    );

    const drop_goal = useCallback(
        async (repository_id: string) => {
            try {
                await clear_goal(repository_id);
                set_notice(null);
                await refresh();
            } catch (cause) {
                set_notice(cause instanceof Error ? cause.message : String(cause));
            }
        },
        [refresh],
    );

    const ignite = useCallback(
        async (repository_id: string) => {
            set_igniting(repository_id);
            set_notice(null);
            try {
                const done = await ignite_commander(repository_id);
                if (done.commander.session_id) {
                    open_session(done.commander.session_id);
                }
                await refresh();
            } catch (cause) {
                set_notice(cause instanceof Error ? cause.message : String(cause));
            } finally {
                set_igniting(null);
            }
        },
        [open_session, refresh],
    );

    const running = plans.filter((plan) => plan.state === "running");
    const watching = watches.filter((watch) => watch.state === "working");
    const settled = watches.filter((watch) => watch.state !== "working");

    const step_of = (watch: Watch): PlanStep | undefined =>
        plans.find((plan) => plan.id === watch.plan_id)?.steps.find((step) => step.id === watch.step_id);

    return (
        <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-2 overflow-y-auto p-2.5">
            <section className="flex flex-col gap-1.5">
                {mine.length === 0 ? (
                    <span className="font-mono text-[11px] text-shade">
                        No project in this workspace yet.
                    </span>
                ) : null}

                {mine.map((repo) => {
                    const held = commander_of(repo.id);
                    const at_work = Boolean(held?.session_id);

                    const goal = goals.find((held) => held.repository_id === repo.id);

                    return (
                        <div key={repo.id} className="flex flex-col gap-1">
                        <div className="flex flex-wrap items-center gap-2">
                            <span className="font-mono text-[11px] text-linen">{repo.name}</span>
                            <span className="font-mono text-[10px] text-shade">
                                {held
                                    ? `${held.name} · ${at_work ? "at its desk" : "stopped"}`
                                    : "no commander yet"}
                            </span>

                            {igniting === repo.id ? (
                                <Waiting says="starting…" className="font-mono text-[11px] text-turquoise" />
                            ) : (
                                <button
                                    className="rounded-md border border-turquoise px-2 py-0.5 font-mono text-[11px] text-turquoise"
                                    onClick={() => ignite(repo.id)}
                                    title={
                                        held
                                            ? "hand it the project again"
                                            : "hire X here and set it going"
                                    }
                                >
                                    {at_work ? `tell ${held?.name}` : `start ${held?.name ?? "X"}`}
                                </button>
                            )}

                            {at_work && held?.session_id ? (
                                <button
                                    className="rounded-md border border-reef px-2 py-0.5 font-mono text-[11px] text-shell hover:border-foam"
                                    onClick={() => held.session_id && open_session(held.session_id)}
                                >
                                    open its pane
                                </button>
                            ) : null}
                        </div>

                        {writing === repo.id ? (
                            <div className="flex flex-wrap items-center gap-2 pl-1">
                                <input
                                    className="min-w-0 flex-1 rounded-md border border-reef bg-lagoon px-2 py-1 font-mono text-[11px]"
                                    autoFocus
                                    placeholder="what this project is for, in your words"
                                    value={draft}
                                    onChange={(event) => set_draft(event.target.value)}
                                    onKeyDown={(event) => {
                                        if (event.key === "Enter") {
                                            void save_goal(repo.id);
                                        }
                                        if (event.key === "Escape") {
                                            set_writing(null);
                                        }
                                    }}
                                />
                                <button
                                    className="rounded-md border border-turquoise px-2 py-0.5 font-mono text-[11px] text-turquoise"
                                    onClick={() => void save_goal(repo.id)}
                                >
                                    set
                                </button>
                            </div>
                        ) : (
                            <div className="flex flex-wrap items-baseline gap-2 pl-1">
                                <span className="min-w-0 flex-1 font-mono text-[10px] text-shell">
                                    {goal ? `“${goal.text}”` : "no goal standing — X will read the project and ask"}
                                </span>
                                <button
                                    className="shrink-0 font-mono text-[10px] text-shade hover:text-turquoise"
                                    onClick={() => {
                                        set_draft(goal?.text ?? "");
                                        set_writing(repo.id);
                                    }}
                                >
                                    {goal ? "change it" : "set a goal"}
                                </button>
                                {goal ? (
                                    <button
                                        className="shrink-0 font-mono text-[10px] text-shade hover:text-coral"
                                        onClick={() => void drop_goal(repo.id)}
                                        title="it is done, or it was never the thing"
                                    >
                                        it is done
                                    </button>
                                ) : null}
                            </div>
                        )}
                        </div>
                    );
                })}

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

            <section className="shrink-0">
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
