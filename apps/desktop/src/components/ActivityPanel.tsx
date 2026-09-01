import { useCallback, useState } from "react";

import { Waiting } from "@/components/Spinner";
import {
    read_budget,
    read_journal,
    set_ceilings,
    type Budget,
    type JournalEntry,
    type Room,
} from "@/lib/core";
import { families_in, family_of, meters_of, moments_ago, short_count } from "@/lib/activity";
import { use_poll } from "@/lib/poll";

const ROOM_TINT: Record<Room, string> = {
    plenty: "text-palm",
    tight: "text-sun",
    spent: "text-coral",
};

const FAMILY_TINT: Record<string, string> = {
    card: "text-turquoise",
    step: "text-palm",
    commander: "text-sun",
    engine: "text-coral",
    pull: "text-driftwood",
    agent: "text-shell",
    brief: "text-shell",
    project: "text-shell",
};

/// What the crew is spending, and what it has been doing.
///
/// The two belong together: the journal says what happened and the budget says
/// what it cost. Reading either alone leaves the other question open — an app
/// that spent a week's allowance and cannot say on what has only half a story.
export function ActivityPanel({ active }: { active: boolean }) {
    const [budget, set_budget] = useState<Budget | null>(null);
    const [entries, set_entries] = useState<JournalEntry[]>([]);
    const [family, set_family] = useState<string | null>(null);
    const [editing, set_editing] = useState(false);
    const [draft, set_draft] = useState({ requests: "", input: "", output: "" });
    const [notice, set_notice] = useState<string | null>(null);
    const [now, set_now] = useState(() => Math.floor(Date.now() / 1000));

    const refresh = useCallback(async () => {
        const [held, log] = await Promise.all([
            read_budget(),
            read_journal(family ? { kind: family } : {}),
        ]);
        set_budget(held);
        set_entries(log);
        set_now(Math.floor(Date.now() / 1000));
    }, [family]);

    use_poll(() => {
        refresh().catch((cause) => set_notice(cause instanceof Error ? cause.message : String(cause)));
    }, 5000, active);

    const save = useCallback(async () => {
        try {
            const wanted = {
                requests: Number(draft.requests),
                input: Number(draft.input),
                output: Number(draft.output),
            };

            if (Object.values(wanted).some((value) => !Number.isFinite(value) || value <= 0)) {
                set_notice("a ceiling is a number above zero");
                return;
            }

            await set_ceilings(wanted);
            set_editing(false);
            set_notice(null);
            await refresh();
        } catch (cause) {
            set_notice(cause instanceof Error ? cause.message : String(cause));
        }
    }, [draft, refresh]);

    // The families come from what is actually in the journal rather than a list
    // written here, so a kind added to the core shows up without being added twice.
    const families = families_in(entries);
    const meters = budget ? meters_of(budget.last_minute, budget.ceilings) : [];

    return (
        <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-2.5 overflow-y-auto p-2.5">
            <section className="flex flex-col gap-2 rounded-lg border border-reef bg-lagoon-deep p-2">
                <div className="flex flex-wrap items-baseline justify-between gap-2">
                    <h3 className="font-mono text-[9px] uppercase tracking-[0.14em] text-shade">
                        What it may spend
                    </h3>
                    {budget ? (
                        <span className={`font-mono text-[11px] ${ROOM_TINT[budget.room]}`}>
                            {budget.says}
                        </span>
                    ) : null}
                </div>

                {!budget ? <Waiting says="reading the meters…" className="font-mono text-[11px] text-shade" /> : null}

                {budget ? (
                    <>
                        <p className="font-mono text-[10px] text-shade">
                            {budget.weekly_percent === undefined
                                ? "No engine has said what the account has left yet — it is read off a pane's own status line."
                                : `The account: ${budget.weekly_percent}% of the week spent, ${budget.session_percent}% of this session, read ${moments_ago(now - (budget.read_seconds_ago ?? 0), now)} ago.`}
                        </p>

                        <div className="flex flex-col gap-1">
                            {meters.map((meter) => (
                                <div key={meter.label} className="flex items-center gap-2">
                                    <span className="w-24 shrink-0 font-mono text-[10px] text-shade">
                                        {meter.label}
                                    </span>
                                    <span className="h-1.5 min-w-0 flex-1 rounded-full bg-lagoon">
                                        <span
                                            className={`block h-1.5 rounded-full ${meter.tightest ? "bg-sun" : "bg-turquoise"}`}
                                            style={{ width: `${Math.round(meter.share * 100)}%` }}
                                        />
                                    </span>
                                    <span
                                        className={`w-28 shrink-0 text-right font-mono text-[10px] ${meter.tightest ? "text-sun" : "text-shell"}`}
                                    >
                                        {short_count(meter.used)} / {short_count(meter.ceiling)}
                                    </span>
                                </div>
                            ))}
                        </div>

                        <p className="font-mono text-[10px] text-shade">
                            Per minute, counted from what the engines wrote. This app makes none of
                            those requests, so the only honest way to count them is to read what the
                            engines recorded.
                        </p>

                        {editing ? (
                            <div className="flex flex-wrap items-center gap-2">
                                {(["requests", "input", "output"] as const).map((key) => (
                                    <input
                                        key={key}
                                        className="w-28 rounded-lg border border-reef bg-lagoon px-2 py-1 font-mono text-[11px]"
                                        placeholder={key}
                                        value={draft[key]}
                                        onChange={(event) =>
                                            set_draft({ ...draft, [key]: event.target.value })
                                        }
                                    />
                                ))}
                                <button
                                    className="rounded-lg border border-turquoise px-2 py-0.5 font-mono text-[11px] text-turquoise"
                                    onClick={() => void save()}
                                >
                                    set
                                </button>
                                <button
                                    className="rounded-lg border border-reef px-2 py-0.5 font-mono text-[11px] text-shell"
                                    onClick={() => set_editing(false)}
                                >
                                    keep them
                                </button>
                            </div>
                        ) : (
                            <button
                                className="self-start font-mono text-[10px] text-shade hover:text-shell"
                                onClick={() => {
                                    set_draft({
                                        requests: String(budget.ceilings.requests),
                                        input: String(budget.ceilings.input),
                                        output: String(budget.ceilings.output),
                                    });
                                    set_editing(true);
                                }}
                            >
                                change the ceilings — they are a fact about your plan, not something
                                this can measure
                            </button>
                        )}
                    </>
                ) : null}
            </section>

            {notice ? <p className="font-mono text-[11px] text-coral">{notice}</p> : null}

            <section className="flex min-h-0 flex-col gap-1.5">
                <div className="flex flex-wrap items-center gap-1.5">
                    <h3 className="mr-1 font-mono text-[9px] uppercase tracking-[0.14em] text-shade">
                        What it did · {entries.length}
                    </h3>
                    <button
                        className={`rounded px-1.5 py-[1px] font-mono text-[10px] ${family === null ? "text-turquoise" : "text-shade hover:text-shell"}`}
                        onClick={() => set_family(null)}
                    >
                        everything
                    </button>
                    {families.map((held) => (
                        <button
                            key={held}
                            className={`rounded px-1.5 py-[1px] font-mono text-[10px] ${family === held ? "text-turquoise" : "text-shade hover:text-shell"}`}
                            onClick={() => set_family(held)}
                        >
                            {held}
                        </button>
                    ))}
                </div>

                {entries.length === 0 ? (
                    <p className="font-mono text-[10px] text-shade">
                        Nothing recorded yet. Every decision the core takes lands here — who was
                        given a card and why, what was held back for want of allowance, each time
                        the commander was woken.
                    </p>
                ) : null}

                <ol className="flex flex-col gap-0.5">
                    {entries.map((entry, index) => (
                        <li
                            key={`${entry.at}-${index}`}
                            className="flex flex-wrap items-baseline gap-2 rounded border border-reef/40 px-1.5 py-1"
                        >
                            <span className="w-9 shrink-0 text-right font-mono text-[10px] text-shade">
                                {moments_ago(entry.at, now)}
                            </span>
                            <span
                                className={`font-mono text-[10px] ${FAMILY_TINT[family_of(entry.kind)] ?? "text-shell"}`}
                            >
                                {entry.kind}
                            </span>
                            <span className="font-mono text-[10px] text-driftwood">{entry.actor}</span>
                            {entry.subject ? (
                                <span className="font-mono text-[10px] text-shade">{entry.subject}</span>
                            ) : null}
                            {entry.detail ? (
                                <span className="min-w-0 flex-1 text-[11px] text-shell">{entry.detail}</span>
                            ) : null}
                        </li>
                    ))}
                </ol>
            </section>
        </div>
    );
}
