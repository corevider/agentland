import { useCallback, useState } from "react";

import { Waiting } from "@/components/Spinner";
import {
    forget_permit,
    read_budget,
    read_journal,
    read_permits,
    set_ceilings,
    type Allowance,
    type Budget,
    type JournalEntry,
    type ProjectPermits,
    type Room,
} from "@/lib/core";
import { families_in, family_of, meters_of, moments_ago, rule_reads, short_count } from "@/lib/activity";
import { use_poll } from "@/lib/poll";

/// One subscription's allowance: what it has left, and what it is spending.
///
/// A block each rather than one set of numbers, because two subscriptions are
/// two weeks. A single global figure meant an exhausted Claude account stopped a
/// Codex agent whose own week had not been touched.
function Spend({
    allowance,
    now,
    editing,
    draft,
    on_draft,
    on_edit,
    on_save,
    on_cancel,
}: {
    allowance: Allowance;
    now: number;
    editing: boolean;
    draft: { requests: string; input: string; output: string };
    on_draft: (next: { requests: string; input: string; output: string }) => void;
    on_edit: () => void;
    on_save: () => void;
    on_cancel: () => void;
}) {
    const meters = meters_of(allowance.last_minute, allowance.ceilings);

    return (
        <section className="flex flex-col gap-2 rounded-lg border border-reef bg-lagoon-deep p-2">
            <div className="flex flex-wrap items-baseline justify-between gap-2">
                <span className="font-mono text-[11px] text-linen">{allowance.identity}</span>
                <span className={`font-mono text-[11px] ${ROOM_TINT[allowance.room]}`}>
                    {allowance.says}
                </span>
            </div>

            <p className="font-mono text-[10px] text-shade">
                {allowance.weekly_percent === undefined
                    ? "No pane on this one has said what it has left yet — it is read off the engine's own status line."
                    : `${allowance.weekly_percent}% of the week spent, ${allowance.session_percent}% of this session, read ${moments_ago(now - (allowance.read_seconds_ago ?? 0), now)} ago.`}
                {allowance.agents.length > 0 ? ` Spent by ${allowance.agents.join(", ")}.` : " Nobody is hired on it."}
            </p>

            <div className="flex flex-col gap-1">
                {meters.map((meter) => (
                    <div key={meter.label} className="flex items-center gap-2">
                        <span className="w-24 shrink-0 font-mono text-[10px] text-shade">{meter.label}</span>
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

            {editing ? (
                <div className="flex flex-wrap items-center gap-2">
                    {(["requests", "input", "output"] as const).map((key) => (
                        <input
                            key={key}
                            className="w-28 rounded-lg border border-reef bg-lagoon px-2 py-1 font-mono text-[11px]"
                            placeholder={key}
                            value={draft[key]}
                            onChange={(event) => on_draft({ ...draft, [key]: event.target.value })}
                        />
                    ))}
                    <button
                        className="rounded-lg border border-turquoise px-2 py-0.5 font-mono text-[11px] text-turquoise"
                        onClick={on_save}
                    >
                        set
                    </button>
                    <button
                        className="rounded-lg border border-reef px-2 py-0.5 font-mono text-[11px] text-shell"
                        onClick={on_cancel}
                    >
                        keep them
                    </button>
                </div>
            ) : (
                <button className="self-start font-mono text-[10px] text-shade hover:text-shell" onClick={on_edit}>
                    per minute, counted from what its engines wrote · change the ceilings
                </button>
            )}
        </section>
    );
}

/// What a project's agents may do without asking, and a way to take it back.
///
/// Saying yes is one click and holds forever; until this there was no matching
/// no. A grant that can only be undone by editing the database is a grant
/// nobody will undo.
function Granted({
    permits,
    on_forget,
}: {
    permits: ProjectPermits;
    on_forget: (rule: string) => void;
}) {
    return (
        <section className="flex flex-col gap-1 rounded-lg border border-reef bg-lagoon-deep p-2">
            <div className="flex flex-wrap items-baseline justify-between gap-2">
                <span className="font-mono text-[11px] text-linen">{permits.repository_id}</span>
                <span className="font-mono text-[10px] text-shade">
                    {permits.running.length > 0
                        ? `${permits.running.join(", ")} running — a pane keeps what it started with`
                        : "no pane open"}
                </span>
            </div>

            <ul className="flex flex-col gap-0.5">
                {permits.rules.map((rule) => (
                    <li key={rule} className="flex items-baseline gap-2">
                        <span className="min-w-0 flex-1 font-mono text-[10px] text-shell">
                            {rule_reads(rule)}
                        </span>
                        <button
                            className="shrink-0 rounded border border-reef px-1.5 py-[1px] font-mono text-[10px] text-shade hover:border-coral hover:text-coral"
                            onClick={() => on_forget(rule)}
                        >
                            take it back
                        </button>
                    </li>
                ))}
            </ul>
        </section>
    );
}

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
    const [permits, set_permits] = useState<ProjectPermits[]>([]);
    const [family, set_family] = useState<string | null>(null);
    const [editing, set_editing] = useState<string | null>(null);
    const [draft, set_draft] = useState({ requests: "", input: "", output: "" });
    const [notice, set_notice] = useState<string | null>(null);
    const [now, set_now] = useState(() => Math.floor(Date.now() / 1000));

    const refresh = useCallback(async () => {
        const [held, log, granted] = await Promise.all([
            read_budget(),
            read_journal(family ? { kind: family } : {}),
            read_permits(),
        ]);
        set_budget(held);
        set_entries(log);
        set_permits(granted);
        set_now(Math.floor(Date.now() / 1000));
    }, [family]);

    use_poll(() => {
        refresh().catch((cause) => set_notice(cause instanceof Error ? cause.message : String(cause)));
    }, 5000, active);

    const save = useCallback(async (identity: string) => {
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

            await set_ceilings(identity, wanted);
            set_editing(null);
            set_notice(null);
            await refresh();
        } catch (cause) {
            set_notice(cause instanceof Error ? cause.message : String(cause));
        }
    }, [draft, refresh]);

    const forget = useCallback(async (repository_id: string, rule: string) => {
        try {
            await forget_permit(repository_id, rule);
            set_notice(null);
            await refresh();
        } catch (cause) {
            set_notice(cause instanceof Error ? cause.message : String(cause));
        }
    }, [refresh]);

    // The families come from what is actually in the journal rather than a list
    // written here, so a kind added to the core shows up without being added twice.
    const families = families_in(entries);

    return (
        <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-2.5 overflow-y-auto p-2.5">
            {!budget ? (
                <Waiting says="reading the meters…" className="font-mono text-[11px] text-shade" />
            ) : null}

            {budget && budget.allowances.length === 0 ? (
                <p className="font-mono text-[11px] text-shade">
                    Nobody is hired yet, so there is no allowance to watch.
                </p>
            ) : null}

            {budget?.allowances.map((allowance) => (
                <Spend
                    key={allowance.identity}
                    allowance={allowance}
                    now={now}
                    editing={editing === allowance.identity}
                    draft={draft}
                    on_draft={set_draft}
                    on_edit={() => {
                        set_draft({
                            requests: String(allowance.ceilings.requests),
                            input: String(allowance.ceilings.input),
                            output: String(allowance.ceilings.output),
                        });
                        set_editing(allowance.identity);
                    }}
                    on_save={() => void save(allowance.identity)}
                    on_cancel={() => set_editing(null)}
                />
            ))}

            {permits.length > 0 ? (
                <section className="flex flex-col gap-1.5">
                    <h3 className="font-mono text-[9px] uppercase tracking-[0.14em] text-shade">
                        Allowed without asking
                    </h3>
                    {permits.map((held) => (
                        <Granted
                            key={held.repository_id}
                            permits={held}
                            on_forget={(rule) => void forget(held.repository_id, rule)}
                        />
                    ))}
                </section>
            ) : null}

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
