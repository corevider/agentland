import { use_poll } from "@/lib/poll";
import { useCallback, useEffect, useState } from "react";

import {
    clear_goal,
    clear_workspace_goal,
    command_the_workspace,
    read_goals,
    set_goal,
    set_workspace_goal,
    ignite_commander,
    list_plans,
    list_repos,
    list_tasks,
    list_workspaces,
    write_input,
    mark_step,
    ready_steps,
    suggest_a_chief,
    supervisor_watches,
    type Plan,
    type PlanStep,
    type ReadyStep,
    type Goal,
    type Repository,
    type Task,
    type Watch,
    type Workspace,
} from "@/lib/core";
import { chief_of, clear_is_recommended, commander_of } from "@/lib/commander";
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
    const [workspace, set_workspace] = useState<Workspace | null>(null);
    const [naming, set_naming] = useState("");
    const [suggested, set_suggested] = useState("");
    const [igniting, set_igniting] = useState<string | null>(null);
    const [notice, set_notice] = useState<string | null>(null);
    const [goals, set_goals] = useState<Goal[]>([]);
    const [tasks, set_tasks] = useState<Task[]>([]);
    const [clearing, set_clearing] = useState<string | null>(null);
    const [writing, set_writing] = useState<string | null>(null);
    const [draft, set_draft] = useState("");

    const refresh = useCallback(async () => {
        const [held, next, watching, known, wanted, cards, workspaces] = await Promise.all([
            list_plans(),
            ready_steps(),
            supervisor_watches(),
            list_repos(),
            read_goals(),
            list_tasks(),
            list_workspaces(),
        ]);
        set_plans(held);
        set_ready(next);
        set_watches(watching);
        set_repos(known);
        set_goals(wanted);
        set_tasks(cards);
        set_workspace(
            workspaces.workspaces.find((entry) => entry.id === workspaces.active) ?? null,
        );
    }, []);

    use_poll(() => {
        refresh().catch(() => undefined);
    }, 4000, active);

    // A commander belongs to a project. Reading the first one in the crew showed
    // one project's X while standing in another's, which is the sort of wrong
    // that looks right.
    const mine = repositories ? repos.filter((repo) => repositories.includes(repo.id)) : repos;
    // The chief commands the workspace, and the commanders below it command a
    // project each. Both are found by what they command rather than by role
    // alone: a machine with three workspaces has three chiefs.
    const chief = chief_of(crew, workspace?.id ?? null);

    // What a chief here would be called, for the field that offers it. Only
    // asked while there is nobody: a workspace that has one is not naming one.
    useEffect(() => {
        if (!workspace || chief) {
            return;
        }

        suggest_a_chief(workspace.name)
            .then((held) => set_suggested(held.chief))
            .catch(() => undefined);
    }, [workspace, chief]);
    const chief_at_work = Boolean(chief?.session_id);
    const workspace_goal = workspace
        ? goals.find((held) => held.repository_id === workspace.id)
        : undefined;

    const save_goal = useCallback(
        async (id: string, of_workspace = false) => {
            try {
                await (of_workspace ? set_workspace_goal(id, draft) : set_goal(id, draft));
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
        async (id: string, of_workspace = false) => {
            try {
                await (of_workspace ? clear_workspace_goal(id) : clear_goal(id));
                set_notice(null);
                await refresh();
            } catch (cause) {
                set_notice(cause instanceof Error ? cause.message : String(cause));
            }
        },
        [refresh],
    );

    const command = useCallback(
        async (workspace_id: string, name?: string) => {
            set_igniting(workspace_id);
            set_notice(null);
            try {
                const done = await command_the_workspace(workspace_id, undefined, name);
                if (done.chief.session_id) {
                    open_session(done.chief.session_id);
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
            {workspace ? (
                <section className="flex flex-col gap-1 rounded-md border border-reef/70 px-2 py-1.5">
                    <div className="flex flex-wrap items-center gap-2">
                        <span className="font-mono text-[9px] uppercase tracking-[0.14em] text-turquoise">
                            Workspace
                        </span>
                        <span className="font-mono text-[11px] text-linen">{workspace.name}</span>
                        <span className="font-mono text-[10px] text-shade">
                            {chief
                                ? `${chief.name} · ${chief_at_work ? "at its desk" : "stopped"}`
                                : "no chief yet"}
                        </span>

                        {!chief ? (
                            <input
                                className="w-28 rounded-md border border-reef bg-lagoon px-2 py-0.5 font-mono text-[11px]"
                                placeholder={suggested ? `chief · ${suggested}` : "chief"}
                                title="who commands this workspace — leave it be to take the name offered"
                                value={naming}
                                onChange={(event) => set_naming(event.target.value)}
                                onKeyDown={(event) => {
                                    if (event.key === "Enter") {
                                        void command(workspace.id, naming.trim() || suggested);
                                    }
                                }}
                            />
                        ) : null}

                        {igniting === workspace.id ? (
                            <Waiting says="starting…" className="font-mono text-[11px] text-turquoise" />
                        ) : (
                            <button
                                className="rounded-md border border-turquoise px-2 py-0.5 font-mono text-[11px] text-turquoise"
                                onClick={() =>
                                    command(workspace.id, chief ? undefined : naming.trim() || suggested)
                                }
                                title={
                                    chief
                                        ? "hand it the workspace again"
                                        : "hire a chief here and set it going"
                                }
                            >
                                {chief_at_work
                                    ? `tell ${chief?.name}`
                                    : `start ${chief?.name ?? (naming.trim() || suggested || "a chief")}`}
                            </button>
                        )}

                        {chief_at_work && chief?.session_id ? (
                            <button
                                className="rounded-md border border-reef px-2 py-0.5 font-mono text-[11px] text-shell hover:border-foam"
                                onClick={() => chief.session_id && open_session(chief.session_id)}
                            >
                                open its pane
                            </button>
                        ) : null}
                    </div>

                    {writing === workspace.id ? (
                        <div className="flex flex-wrap items-center gap-2 pl-1">
                            <input
                                className="min-w-0 flex-1 rounded-md border border-reef bg-lagoon px-2 py-1 font-mono text-[11px]"
                                autoFocus
                                placeholder="what this whole workspace is for, in your words"
                                value={draft}
                                onChange={(event) => set_draft(event.target.value)}
                                onKeyDown={(event) => {
                                    if (event.key === "Enter") {
                                        void save_goal(workspace.id, true);
                                    }
                                    if (event.key === "Escape") {
                                        set_writing(null);
                                    }
                                }}
                            />
                            <button
                                className="rounded-md border border-turquoise px-2 py-0.5 font-mono text-[11px] text-turquoise"
                                onClick={() => void save_goal(workspace.id, true)}
                            >
                                set
                            </button>
                        </div>
                    ) : (
                        <div className="flex flex-wrap items-baseline gap-2 pl-1">
                            <span className="min-w-0 flex-1 font-mono text-[10px] text-shell">
                                {workspace_goal
                                    ? `“${workspace_goal.text}”`
                                    : "no goal standing — the chief reads the projects and asks"}
                            </span>
                            <button
                                className="shrink-0 font-mono text-[10px] text-shade hover:text-turquoise"
                                onClick={() => {
                                    set_draft(workspace_goal?.text ?? "");
                                    set_writing(workspace.id);
                                }}
                            >
                                {workspace_goal ? "change it" : "set a goal"}
                            </button>
                            {workspace_goal ? (
                                <button
                                    className="shrink-0 font-mono text-[10px] text-shade hover:text-coral"
                                    onClick={() => void drop_goal(workspace.id, true)}
                                    title="it is done, or it was never the thing"
                                >
                                    it is done
                                </button>
                            ) : null}
                        </div>
                    )}

                    <span className="pl-1 font-mono text-[10px] text-shade">
                        It commands the projects below through their commanders — it writes no code
                        and hires nobody into them.
                    </span>
                </section>
            ) : null}

            <section className="flex flex-col gap-1.5">
                {mine.length === 0 ? (
                    <span className="font-mono text-[11px] text-shade">
                        No project in this workspace yet.
                    </span>
                ) : null}

                {mine.map((repo) => {
                    const held = commander_of(crew, repo.id);
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

                            {at_work && held?.session_id
                                ? (() => {
                                      const mine = tasks.filter((task) => task.assignee === held.id);
                                      const recommended = clear_is_recommended({
                                          has_pane: true,
                                          running_plans: running.filter((plan) => plan.repository_id === repo.id).length,
                                          open_cards: mine.filter((task) =>
                                              task.column === "assigned" || task.column === "working",
                                          ).length,
                                          finished_anything:
                                              mine.some((task) => task.column !== "backlog") ||
                                              plans.some(
                                                  (plan) => plan.repository_id === repo.id && plan.state === "done",
                                              ),
                                      });
                                      const session_id = held.session_id;
                                      return (
                                          <button
                                              className={`rounded-md border px-2 py-0.5 font-mono text-[11px] ${
                                                  recommended
                                                      ? "border-sun text-sun hover:border-foam"
                                                      : "border-reef text-shade hover:border-foam hover:text-shell"
                                              }`}
                                              disabled={clearing === repo.id}
                                              title="types /clear into its pane; the next brief carries its identity, the crew and what the project remembers again"
                                              onClick={() => {
                                                  set_clearing(repo.id);
                                                  write_input(session_id, "/clear")
                                                      .then(() => new Promise((done) => window.setTimeout(done, 300)))
                                                      .then(() => write_input(session_id, "\r"))
                                                      .catch((cause) =>
                                                          set_notice(cause instanceof Error ? cause.message : String(cause)),
                                                      )
                                                      .finally(() => set_clearing(null));
                                              }}
                                          >
                                              {clearing === repo.id ? "clearing…" : recommended ? "clear chat · recommended" : "clear chat"}
                                          </button>
                                      );
                                  })()
                                : null}
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
