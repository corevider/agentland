import { useCallback, useEffect, useState } from "react";

import {
    create_routine,
    delete_routine,
    format_elapsed,
    list_routines,
    set_routine_enabled,
    type Routine,
} from "@/lib/core";
import { use_services } from "@/workspace/registry";

export function RoutinesPanel({ active }: { active: boolean }) {
    const { crew } = use_services();
    const [routines, set_routines] = useState<Routine[]>([]);
    const [now, set_now] = useState(() => Math.floor(Date.now() / 1000));
    const [draft, set_draft] = useState({
        name: "",
        agent_id: "",
        brief: "",
        every_minutes: 60,
        draft_only: true,
    });
    const [notice, set_notice] = useState<string | null>(null);

    const refresh = useCallback(async () => {
        set_routines(await list_routines());
    }, []);

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

    const run = useCallback(
        (action: () => Promise<unknown>) => {
            set_notice(null);
            action()
                .then(() => refresh())
                .catch((cause) => set_notice(cause instanceof Error ? cause.message : String(cause)));
        },
        [refresh],
    );

    const create = useCallback(() => {
        const name = draft.name.trim();
        const brief = draft.brief.trim();
        const agent_id = draft.agent_id || crew[0]?.id;

        if (!name || !brief || !agent_id) {
            set_notice("a routine needs a name, an agent and a brief");
            return;
        }

        run(async () => {
            await create_routine({ ...draft, name, brief, agent_id });
            set_draft({ ...draft, name: "", brief: "" });
        });
    }, [crew, draft, run]);

    return (
        <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-2 overflow-y-auto p-2.5">
            <section className="flex flex-wrap items-center gap-1.5">
                <input
                    className="w-28 rounded-md border border-reef bg-lagoon-deep font-mono text-[11px]"
                    placeholder="name"
                    value={draft.name}
                    onChange={(event) => set_draft({ ...draft, name: event.target.value })}
                />
                <select
                    className="rounded-md border border-reef bg-lagoon-deep font-mono text-[11px]"
                    value={draft.agent_id || crew[0]?.id || ""}
                    onChange={(event) => set_draft({ ...draft, agent_id: event.target.value })}
                >
                    {crew.map((agent) => (
                        <option key={agent.id} value={agent.id}>
                            {agent.name}
                        </option>
                    ))}
                </select>
                <input
                    className="min-w-[140px] flex-1 rounded-md border border-reef bg-lagoon-deep font-mono text-[11px]"
                    placeholder="what it should do each time"
                    value={draft.brief}
                    onChange={(event) => set_draft({ ...draft, brief: event.target.value })}
                    onKeyDown={(event) => event.key === "Enter" && create()}
                />
                <label className="flex items-center gap-1 font-mono text-[11px] text-shell">
                    every
                    <input
                        type="number"
                        min={1}
                        className="w-14 rounded-md border border-reef bg-lagoon-deep font-mono text-[11px]"
                        value={draft.every_minutes}
                        onChange={(event) =>
                            set_draft({ ...draft, every_minutes: Number(event.target.value) || 1 })
                        }
                    />
                    min
                </label>
                <label
                    className="flex items-center gap-1 font-mono text-[11px] text-shell"
                    title="a draft routine prepares work without starting the agent"
                >
                    <input
                        type="checkbox"
                        checked={draft.draft_only}
                        onChange={(event) => set_draft({ ...draft, draft_only: event.target.checked })}
                    />
                    draft only
                </label>
                <button
                    className="rounded-md border border-turquoise px-2 py-0.5 font-mono text-[11px] text-turquoise"
                    onClick={create}
                >
                    add
                </button>
            </section>

            {notice ? (
                <div className="rounded-md border border-coral px-2 py-1 font-mono text-[11px] text-coral">
                    {notice}
                </div>
            ) : null}

            <section className="min-h-0">
                {routines.length === 0 ? (
                    <p className="font-mono text-[10px] text-shade">
                        No routine yet. A routine gives an agent the same brief on a timer, and disables
                        itself after two failures rather than running into the wall.
                    </p>
                ) : null}

                <div className="flex flex-col gap-1">
                    {routines.map((routine) => {
                        const overdue =
                            routine.last_run > 0 && now - routine.last_run > routine.every_minutes * 60;

                        return (
                            <article
                                key={routine.id}
                                className={`rounded-md border bg-lagoon-deep px-2 py-1 ${
                                    routine.enabled ? "border-reef" : "border-shade/50"
                                }`}
                            >
                                <div className="flex flex-wrap items-baseline gap-2">
                                    <span className="text-[12px] text-linen">{routine.name}</span>
                                    <span className="font-mono text-[10px] text-shade">
                                        {routine.agent_id} · every {routine.every_minutes} min
                                        {routine.draft_only ? " · draft only" : ""}
                                    </span>
                                    <span className="ml-auto flex items-center gap-1">
                                        <button
                                            className={`rounded border px-1.5 font-mono text-[10px] ${
                                                routine.enabled
                                                    ? "border-palm text-palm"
                                                    : "border-shade text-shade"
                                            }`}
                                            onClick={() =>
                                                run(() => set_routine_enabled(routine.id, !routine.enabled))
                                            }
                                        >
                                            {routine.enabled ? "on" : "off"}
                                        </button>
                                        <button
                                            className="rounded border border-reef px-1.5 font-mono text-[10px] hover:border-coral hover:text-coral"
                                            onClick={() => run(() => delete_routine(routine.id))}
                                        >
                                            delete
                                        </button>
                                    </span>
                                </div>

                                <div className="mt-0.5 text-[11px] text-driftwood">{routine.brief}</div>

                                <div className="mt-0.5 flex flex-wrap gap-2 font-mono text-[10px] text-shade">
                                    <span>
                                        {routine.last_run === 0
                                            ? "never run"
                                            : `last ran ${format_elapsed(now - routine.last_run)} ago`}
                                    </span>
                                    {overdue && routine.enabled ? <span className="text-sun">due</span> : null}
                                    {routine.consecutive_failures > 0 ? (
                                        <span className="text-coral">
                                            {routine.consecutive_failures} failure
                                            {routine.consecutive_failures === 1 ? "" : "s"} in a row
                                        </span>
                                    ) : null}
                                    {routine.last_result ? <span>{routine.last_result}</span> : null}
                                </div>
                            </article>
                        );
                    })}
                </div>
            </section>
        </div>
    );
}
