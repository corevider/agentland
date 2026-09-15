import { useCallback, useEffect, useMemo, useState } from "react";

import { due_in, exactly, when } from "@/lib/when";

import {
    create_routine,
    delete_routine,
    list_routines,
    list_workspaces,
    routine_templates,
    run_routine,
    set_routine_enabled,
    update_routine,
    type Routine,
    type RoutineTemplates,
    type Workspace,
} from "@/lib/core";
import { belongs_here, place_routines, type PlacedRoutine } from "@/lib/places";
import { EMPTY_DRAFT, draft_from_routine, payload_of, schedule_in_words, type RoutineDraft } from "@/lib/routines";
import { use_services } from "@/workspace/registry";
import { RoutineEditor } from "@/components/RoutineEditor";

const OUTCOME_COLOUR: Record<Routine["history"][number]["outcome"], string> = {
    ran: "text-palm",
    failed: "text-coral",
    skipped: "text-shade",
};

export function RoutinesPanel({ active }: { active: boolean }) {
    const { crew, repositories, workspace_id } = use_services();
    const [routines, set_routines] = useState<Routine[]>([]);
    const [workspaces, set_workspaces] = useState<Workspace[]>([]);
    const [kit, set_kit] = useState<RoutineTemplates>({ templates: [], variables: [] });
    const [now, set_now] = useState(() => Math.floor(Date.now() / 1000));
    /// "new", a routine's id, or nothing being written.
    const [editing, set_editing] = useState<string | null>(null);
    const [history_of, set_history_of] = useState<string | null>(null);
    const [notice, set_notice] = useState<string | null>(null);

    const refresh = useCallback(async () => {
        set_routines(await list_routines());
    }, []);

    // Names for the workspaces a routine's agent might command. A chief carries
    // a workspace id and no project, and an id is not what anybody calls it.
    useEffect(() => {
        if (!active) {
            return;
        }

        list_workspaces()
            .then((listed) => set_workspaces(listed.workspaces))
            .catch(() => undefined);
    }, [active]);

    // The templates and the words a brief may carry come from the core, so
    // the panel and the crew's own tools offer the same ones.
    useEffect(() => {
        if (!active || kit.templates.length > 0) {
            return;
        }

        routine_templates()
            .then(set_kit)
            .catch(() => undefined);
    }, [active, kit.templates.length]);

    // Only the agents in this workspace can be given a routine from here. The
    // rest are somebody else's, and picking one by accident makes a routine
    // that runs where nobody is looking.
    const mine = useMemo(
        () => crew.filter((agent) => belongs_here(agent, workspace_id, repositories)),
        [crew, repositories, workspace_id],
    );
    const choices = useMemo(() => mine.map((agent) => ({ value: agent.id, label: agent.name })), [mine]);

    const placed = useMemo(
        () => place_routines(routines, crew, workspaces, workspace_id, repositories),
        [crew, repositories, routines, workspace_id, workspaces],
    );

    const name_of = useCallback((id: string) => crew.find((agent) => agent.id === id)?.name ?? id, [crew]);

    useEffect(() => {
        if (!active) {
            return;
        }

        refresh().catch((cause) => set_notice(cause instanceof Error ? cause.message : String(cause)));
        const handle = window.setInterval(() => {
            set_now(Math.floor(Date.now() / 1000));
            refresh().catch(() => undefined);
        }, 5000);
        return () => window.clearInterval(handle);
    }, [active, refresh]);

    const act = useCallback(
        (action: () => Promise<unknown>) => {
            set_notice(null);
            action()
                .then(() => refresh())
                .catch((cause) => set_notice(cause instanceof Error ? cause.message : String(cause)));
        },
        [refresh],
    );

    const save = useCallback(
        (draft: RoutineDraft) => {
            const payload = payload_of(draft);
            const writing = editing;

            act(async () => {
                if (writing === "new") {
                    await create_routine(payload);
                } else if (writing) {
                    await update_routine(writing, payload);
                }
                set_editing(null);
            });
        },
        [act, editing],
    );

    /// Where a routine stands, in the words its card leads with.
    const standing = useCallback(
        (routine: Routine, who: string): { says: string; title?: string; colour: string } => {
            if (!routine.enabled && routine.created_by) {
                return {
                    says: `proposed by ${name_of(routine.created_by)} — waiting for you to turn it on`,
                    colour: "text-sun",
                };
            }
            if (!routine.enabled) {
                return { says: "paused", colour: "text-shade" };
            }
            if (routine.waiting_since > 0) {
                return { says: `waiting for ${who} to come to rest`, colour: "text-sun" };
            }
            if (routine.next_run === null) {
                return { says: "not scheduled", colour: "text-shade" };
            }
            return {
                says: routine.next_run <= now ? "due now" : `next ${due_in(routine.next_run, now)}`,
                title: exactly(routine.next_run),
                colour: "text-shell",
            };
        },
        [name_of, now],
    );

    const card = useCallback(
        ({ routine, who, where, gone }: PlacedRoutine<Routine>) => {
            if (editing === routine.id) {
                const agents = choices.some((choice) => choice.value === routine.agent_id)
                    ? choices
                    : [{ value: routine.agent_id, label: who }, ...choices];

                return (
                    <RoutineEditor
                        key={routine.id}
                        initial={draft_from_routine(routine)}
                        agents={agents}
                        templates={[]}
                        variables={kit.variables}
                        saving_label="save"
                        on_save={save}
                        on_cancel={() => set_editing(null)}
                    />
                );
            }

            const stands = standing(routine, who);
            const proposed = !routine.enabled && routine.created_by !== null;
            const history_open = history_of === routine.id;

            return (
                <article
                    key={routine.id}
                    className={`rounded-md border bg-lagoon-deep px-2 py-1 ${
                        proposed ? "border-sun/60" : routine.enabled ? "border-reef" : "border-shade/50"
                    }`}
                >
                    <div className="flex flex-wrap items-baseline gap-2">
                        <span className="text-[12px] text-linen">{routine.name}</span>
                        <span className="font-mono text-[10px] text-driftwood">{schedule_in_words(routine.schedule)}</span>
                        <span className="font-mono text-[10px] text-shade">
                            {routine.delivery === "pane" ? "into its pane" : "on a card"}
                            {routine.draft_only ? " · draft only" : ""}
                        </span>
                        <span className="ml-auto flex items-center gap-1">
                            <button
                                className="rounded border border-reef px-1.5 font-mono text-[10px] text-shell hover:border-turquoise hover:text-turquoise"
                                title="run it now, whatever the schedule says"
                                onClick={() => act(() => run_routine(routine.id))}
                            >
                                run now
                            </button>
                            <button
                                className="rounded border border-reef px-1.5 font-mono text-[10px] text-shell hover:border-foam"
                                onClick={() => set_editing(routine.id)}
                            >
                                edit
                            </button>
                            <button
                                className={`rounded border px-1.5 font-mono text-[10px] ${
                                    proposed
                                        ? "border-sun text-sun"
                                        : routine.enabled
                                          ? "border-palm text-palm"
                                          : "border-shade text-shade"
                                }`}
                                onClick={() => act(() => set_routine_enabled(routine.id, !routine.enabled))}
                            >
                                {proposed ? "turn on" : routine.enabled ? "on" : "off"}
                            </button>
                            <button
                                className="rounded border border-reef px-1.5 font-mono text-[10px] hover:border-coral hover:text-coral"
                                onClick={() => act(() => delete_routine(routine.id))}
                            >
                                delete
                            </button>
                        </span>
                    </div>

                    <div className="font-mono text-[10px] text-shade">
                        {who} · <span className={gone ? "text-coral" : "text-shade"}>{where}</span>
                    </div>

                    <div className="mt-0.5 line-clamp-2 whitespace-pre-line text-[11px] text-driftwood" title={routine.brief}>
                        {routine.brief}
                    </div>

                    <div className="mt-0.5 flex flex-wrap gap-2 font-mono text-[10px] text-shade">
                        <span className={stands.colour} title={stands.title}>
                            {stands.says}
                        </span>
                        <span title={exactly(routine.last_run)}>
                            {routine.last_run === 0 ? "never run" : `last ran ${when(routine.last_run, now)}`}
                        </span>
                        {routine.consecutive_failures > 0 ? (
                            <span className="text-coral">
                                {routine.consecutive_failures} failure
                                {routine.consecutive_failures === 1 ? "" : "s"} in a row · pauses at{" "}
                                {routine.pause_after_failures}
                            </span>
                        ) : null}
                        {routine.last_result ? <span>{routine.last_result}</span> : null}
                        {routine.history.length > 0 ? (
                            <button
                                className="ml-auto text-shade hover:text-shell"
                                onClick={() => set_history_of(history_open ? null : routine.id)}
                            >
                                {history_open ? "hide runs" : `${routine.history.length} run${routine.history.length === 1 ? "" : "s"}`}
                            </button>
                        ) : null}
                    </div>

                    {history_open ? (
                        <ol className="mt-1 flex flex-col gap-0.5 border-t border-reef/60 pt-1">
                            {routine.history.map((run) => (
                                <li key={`${run.at}-${run.outcome}`} className="flex gap-2 font-mono text-[10px]">
                                    <span className="w-16 shrink-0 text-shade" title={exactly(run.at)}>
                                        {when(run.at, now)}
                                    </span>
                                    <span className={`w-12 shrink-0 ${OUTCOME_COLOUR[run.outcome]}`}>{run.outcome}</span>
                                    <span className="text-driftwood">{run.detail}</span>
                                </li>
                            ))}
                        </ol>
                    ) : null}
                </article>
            );
        },
        [act, choices, editing, history_of, kit.variables, now, save, standing],
    );

    return (
        <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-2 overflow-y-auto p-2.5">
            <section className="flex flex-wrap items-center gap-2">
                <button
                    className="rounded-md border border-turquoise px-2 py-0.5 font-mono text-[11px] text-turquoise disabled:border-shade disabled:text-shade"
                    disabled={mine.length === 0}
                    title={mine.length === 0 ? "no agent in this workspace yet — Crew is where one is hired" : undefined}
                    onClick={() => set_editing(editing === "new" ? null : "new")}
                >
                    {editing === "new" ? "close" : "+ new routine"}
                </button>
                <span className="font-mono text-[10px] text-shade">
                    {routines.filter((routine) => routine.enabled).length} on ·{" "}
                    {routines.filter((routine) => !routine.enabled && routine.created_by).length} proposed ·{" "}
                    {routines.length} in all
                </span>
            </section>

            {editing === "new" ? (
                <RoutineEditor
                    key="new"
                    initial={{ ...EMPTY_DRAFT, agent_id: mine[0]?.id ?? "" }}
                    agents={choices}
                    templates={kit.templates}
                    variables={kit.variables}
                    saving_label="add routine"
                    on_save={save}
                    on_cancel={() => set_editing(null)}
                />
            ) : null}

            {notice ? (
                <div className="rounded-md border border-coral px-2 py-1 font-mono text-[11px] text-coral">{notice}</div>
            ) : null}

            <section className="shrink-0">
                {routines.length === 0 ? (
                    <p className="font-mono text-[10px] text-shade">
                        No routine yet. A routine hands an agent the same brief on a schedule — at set times or
                        every so often, on a card or into its pane — and pauses itself after failures rather than
                        running into the wall. Start from a template with + new routine.
                    </p>
                ) : placed.here.length === 0 ? (
                    <p className="font-mono text-[10px] text-shade">Nothing runs on a timer in this workspace.</p>
                ) : null}

                <div className="flex flex-col gap-1">
                    {placed.here.length > 0 && placed.elsewhere.length > 0 ? (
                        <span className="mt-1 font-mono text-[9px] uppercase tracking-[0.14em] text-shade">here</span>
                    ) : null}
                    {placed.here.map(card)}

                    {placed.elsewhere.length > 0 ? (
                        <span
                            className="mt-1 font-mono text-[9px] uppercase tracking-[0.14em] text-shade"
                            title="a routine you cannot see is one you cannot turn off, so the ones from other workspaces stay listed"
                        >
                            elsewhere · {placed.elsewhere.length}
                        </span>
                    ) : null}
                    {placed.elsewhere.map(card)}
                </div>
            </section>
        </div>
    );
}
