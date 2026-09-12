import { useCallback, useEffect, useState } from "react";

import { AnnotatedPatch } from "./AnnotatedPatch";
import {
    call_off_race,
    keep_entrant,
    list_engines,
    review_worktree,
    start_race,
    type Agent,
    type Engine,
    type Race,
    type Review,
    type Task,
} from "@/lib/core";
import { use_poll } from "@/lib/poll";
import {
    FEWEST,
    MODELS_FOR,
    MOST,
    another_lane,
    first_lanes,
    lane_words,
    standing_of,
    type LaneDraft,
} from "@/lib/races";

const BUTTON = "rounded-lg border px-2 py-0.5 font-mono text-[11px] disabled:opacity-40";

const STANDING_TINT: Record<string, string> = {
    working: "text-sun",
    waiting: "text-palm",
    idle: "text-palm",
    done: "text-palm",
    attention: "text-coral",
    gone: "text-shade",
};

const message_of = (cause: unknown) => (cause instanceof Error ? cause.message : String(cause));

/// Choosing who races a card: an engine and a model per entrant.
///
/// Starts from two lanes that differ, since a race between two copies of the
/// same agent tells nobody anything.
export function RaceStarter({ task, on_started }: { task: Task; on_started: (race: Race) => void }) {
    const [open, set_open] = useState(false);
    const [engines, set_engines] = useState<Engine[] | null>(null);
    const [lanes, set_lanes] = useState<LaneDraft[]>([]);
    const [busy, set_busy] = useState(false);
    const [error, set_error] = useState<string | null>(null);

    useEffect(() => {
        if (!open || engines) {
            return;
        }
        list_engines()
            .then((listed) => {
                set_engines(listed);
                set_lanes(first_lanes(listed));
            })
            .catch((cause) => set_error(message_of(cause)));
    }, [open, engines]);

    if (!open) {
        return (
            <button
                className={`${BUTTON} border-sun text-sun`}
                title="hand this card to several agents at once, each in a worktree of its own, and keep the best result"
                onClick={() => set_open(true)}
            >
                race it
            </button>
        );
    }

    const installed = (engines ?? []).filter((engine) => engine.installed);
    const next = another_lane(lanes, installed);
    const change = (index: number, lane: LaneDraft) =>
        set_lanes((held) => held.map((was, at) => (at === index ? lane : was)));

    const start = () => {
        set_busy(true);
        set_error(null);
        start_race(
            task.id,
            lanes.map((lane) => ({ engine_id: lane.engine_id, model: lane.model.trim() || null })),
        )
            .then((race) => {
                set_open(false);
                on_started(race);
            })
            .catch((cause) => set_error(message_of(cause)))
            .finally(() => set_busy(false));
    };

    return (
        <section data-race-starter className="w-full rounded-md border border-sun/60 bg-lagoon-deep px-2 py-1.5">
            <h3 className="font-mono text-[9px] uppercase tracking-[0.14em] text-sun">Race · {lanes.length} entrants</h3>
            <p className="font-mono text-[9px] text-shade">
                each gets a worktree of its own and this card's brief · you compare what comes back and keep one
            </p>

            {lanes.map((lane, index) => (
                <div key={index} className="mt-1 flex items-center gap-1">
                    <select
                        aria-label={`entrant ${index + 1} engine`}
                        className="rounded-md border border-reef bg-lagoon px-1 py-0.5 font-mono text-[11px] text-linen"
                        value={lane.engine_id}
                        onChange={(event) =>
                            change(index, {
                                engine_id: event.target.value,
                                model: MODELS_FOR[event.target.value]?.[0] ?? "",
                            })
                        }
                    >
                        {installed.map((engine) => (
                            <option key={engine.id} value={engine.id}>
                                {engine.name}
                            </option>
                        ))}
                    </select>
                    <input
                        aria-label={`entrant ${index + 1} model`}
                        className="min-w-0 flex-1 rounded-md border border-reef bg-lagoon px-1 py-0.5 font-mono text-[11px] text-linen"
                        list={`race-models-${lane.engine_id}`}
                        placeholder="the engine's own model"
                        value={lane.model}
                        onChange={(event) => change(index, { ...lane, model: event.target.value })}
                    />
                    <button
                        className="px-1 font-mono text-[11px] text-shade hover:text-coral disabled:invisible"
                        title="take this entrant out"
                        disabled={lanes.length <= FEWEST}
                        onClick={() => set_lanes((held) => held.filter((_, at) => at !== index))}
                    >
                        ✕
                    </button>
                </div>
            ))}

            {Object.entries(MODELS_FOR).map(([engine_id, models]) => (
                <datalist key={engine_id} id={`race-models-${engine_id}`}>
                    {models.map((model) => (
                        <option key={model} value={model} />
                    ))}
                </datalist>
            ))}

            {engines && installed.length === 0 ? (
                <p className="mt-1 font-mono text-[10px] text-coral">no engine is installed to race on</p>
            ) : null}

            <div className="mt-1.5 flex flex-wrap gap-1.5">
                <button
                    className={`${BUTTON} border-reef text-shell`}
                    disabled={lanes.length >= MOST || !next}
                    onClick={() => next && set_lanes([...lanes, next])}
                >
                    + entrant
                </button>
                <button
                    className={`${BUTTON} border-sun text-sun`}
                    disabled={busy || lanes.length < FEWEST}
                    onClick={start}
                >
                    {busy ? "starting the entrants…" : "start the race"}
                </button>
                <button className={`${BUTTON} border-reef text-shell`} disabled={busy} onClick={() => set_open(false)}>
                    cancel
                </button>
            </div>

            {error ? <p className="mt-1 font-mono text-[10px] text-coral">{error}</p> : null}
        </section>
    );
}

/// Every entrant's work side by side, and a way to keep one.
///
/// A diff read alone looks finished; read next to another it shows which one
/// understood the card. Each column is an entrant's worktree against the
/// project's base branch, read again while they work.
export function RaceBoard({
    race,
    title,
    agents,
    on_close,
    on_finished,
}: {
    race: Race;
    title: string;
    agents: Agent[];
    on_close: () => void;
    on_finished: (said: string) => void;
}) {
    const [reviews, set_reviews] = useState<Record<string, Review | string>>({});
    const [keeping, set_keeping] = useState<string | null>(null);
    const [busy, set_busy] = useState(false);
    const [error, set_error] = useState<string | null>(null);
    const open = race.ended_at === null;

    const read_all = useCallback(() => {
        for (const entrant of race.entrants) {
            review_worktree(race.repository_id, entrant.worktree)
                .then((review) => set_reviews((held) => ({ ...held, [entrant.agent_id]: review })))
                .catch((cause) => set_reviews((held) => ({ ...held, [entrant.agent_id]: message_of(cause) })));
        }
    }, [race]);

    useEffect(read_all, [read_all]);
    use_poll(read_all, 8000, open);

    const act = (work: () => Promise<unknown>, said: string) => {
        set_busy(true);
        set_error(null);
        work()
            .then(() => on_finished(said))
            .catch((cause) => set_error(message_of(cause)))
            .finally(() => set_busy(false));
    };

    return (
        <div data-race={race.id} className="flex min-h-0 min-w-0 flex-1 flex-col">
            <header className="flex flex-wrap items-start justify-between gap-2 border-b border-reef px-2 py-1.5">
                <div className="min-w-0">
                    <div className="text-[12px] text-linen">{title}</div>
                    <div className="font-mono text-[10px] text-shade">
                        {race.task_id} · {race.id} · {race.entrants.length} entrants side by side ·{" "}
                        {open ? "keep the one that got it right" : race.winner ? `kept ${race.winner}` : "called off"}
                    </div>
                </div>
                <div className="flex shrink-0 gap-2">
                    {open ? (
                        <button
                            className={`${BUTTON} border-coral text-coral`}
                            disabled={busy}
                            title="let every entrant go and leave the card with nobody"
                            onClick={() =>
                                act(() => call_off_race(race.id), `${race.id} called off — every entrant was let go`)
                            }
                        >
                            call it off
                        </button>
                    ) : null}
                    <button className={`${BUTTON} border-reef text-shell`} onClick={on_close}>
                        close
                    </button>
                </div>
            </header>

            {error ? <p className="px-2 py-1 font-mono text-[10px] text-coral">{error}</p> : null}

            <div
                className="grid min-h-0 flex-1 gap-px overflow-x-auto bg-reef"
                style={{ gridTemplateColumns: `repeat(${race.entrants.length}, minmax(18rem, 1fr))` }}
            >
                {race.entrants.map((entrant) => {
                    const review = reviews[entrant.agent_id];
                    const standing = standing_of(entrant, agents);

                    return (
                        <section
                            key={entrant.agent_id}
                            data-entrant={entrant.agent_id}
                            className="flex min-h-0 min-w-0 flex-col bg-lagoon"
                        >
                            <header className="border-b border-reef px-2 py-1.5">
                                <div className="flex items-baseline justify-between gap-2">
                                    <span className="text-[12px] text-linen">{entrant.name}</span>
                                    <span className={`font-mono text-[10px] ${STANDING_TINT[standing] ?? "text-shell"}`}>
                                        {standing}
                                    </span>
                                </div>
                                <div className="font-mono text-[10px] text-turquoise">{lane_words(entrant)}</div>
                                <div data-stats className="font-mono text-[10px] text-shade">
                                    {typeof review === "object"
                                        ? `${review.files} files · +${review.insertions} −${review.deletions} · ${
                                              review.commits.length
                                          } commit${review.commits.length === 1 ? "" : "s"}${
                                              review.uncommitted ? " · uncommitted work" : ""
                                          }`
                                        : review ?? "reading…"}
                                </div>

                                {open ? (
                                    keeping === entrant.agent_id ? (
                                        <div className="mt-1 flex flex-wrap gap-1">
                                            <button
                                                className={`${BUTTON} border-palm text-palm`}
                                                disabled={busy}
                                                onClick={() =>
                                                    act(
                                                        () => keep_entrant(race.id, entrant.agent_id),
                                                        `kept ${entrant.name}'s work — ${race.task_id} is its now, and the others were let go`,
                                                    )
                                                }
                                            >
                                                keep {entrant.name}, let the rest go
                                            </button>
                                            <button
                                                className={`${BUTTON} border-reef text-shell`}
                                                onClick={() => set_keeping(null)}
                                            >
                                                not yet
                                            </button>
                                        </div>
                                    ) : (
                                        <button
                                            className={`${BUTTON} mt-1 border-palm text-palm`}
                                            disabled={busy}
                                            onClick={() => set_keeping(entrant.agent_id)}
                                        >
                                            keep this one
                                        </button>
                                    )
                                ) : race.winner === entrant.agent_id ? (
                                    <div className="mt-1 font-mono text-[10px] text-palm">kept</div>
                                ) : null}
                            </header>

                            {typeof review === "object" ? (
                                review.patch.trim() ? (
                                    <AnnotatedPatch patch={review.patch} />
                                ) : (
                                    <p className="p-2 font-mono text-[10px] text-shade">nothing changed yet</p>
                                )
                            ) : null}
                        </section>
                    );
                })}
            </div>
        </div>
    );
}
