import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { Press } from "@/components/Press";
import { Waiting } from "@/components/Spinner";
import {
    add_account,
    forget_account,
    list_accounts,
    list_agents,
    read_budget,
    read_journal,
    set_account_failover,
    set_account_rotation,
    sign_in_account,
    type AccountsReport,
    type Agent,
    type Allowance,
    type JournalEntry,
    type RotationChange,
} from "@/lib/core";
import {
    hand_overs,
    login_rows,
    ordinal,
    rank_among,
    reorder,
    week_words,
    who_spends,
    FIVE_HOURS,
    five_hours_words,
    nearness,
    out_words,
    fallback_rows,
    with_fallback,
    without_fallback,
    type LoginRow,
} from "@/lib/logins";
import { exactly, when } from "@/lib/when";
import { Picker } from "@/components/Picker";

const ROOM_COLOUR: Record<Allowance["room"], string> = {
    plenty: "bg-palm",
    tight: "bg-sun",
    spent: "bg-coral",
};

const SWITCH_AT_DEFAULT = 92;
const SESSION_SWITCH_AT_DEFAULT = 95;

/// One allowance as a bar, with the point agents are moved on at marked on it.
function Meter({
    percent,
    point,
    colour,
    says,
    of,
}: {
    percent: number | null | undefined;
    point: number;
    colour: Allowance["room"];
    says: string;
    of: string;
}) {
    const known = percent !== undefined && percent !== null;

    return (
        <span className="flex items-center gap-2">
            <span className="relative h-1.5 w-28 shrink-0 overflow-hidden rounded-full bg-reef/60">
                {known ? (
                    <span
                        className={`block h-full ${ROOM_COLOUR[colour]}`}
                        style={{ width: `${Math.min(100, Math.max(0, percent))}%` }}
                    />
                ) : null}
                <span
                    className="absolute top-0 h-full w-px bg-linen/70"
                    style={{ left: `${point}%` }}
                    title={`agents are moved on at ${point}% of ${of}`}
                />
            </span>
            <span className="font-mono text-[10px] text-shell">{says}</span>
        </span>
    );
}

/// A login's two walls — its week, and the five hours that run out first on a
/// busy day — and who is spending from it.
function Allowances({
    row,
    week_at,
    five_hours_at,
    now,
}: {
    row: LoginRow;
    week_at: number;
    five_hours_at: number;
    now: number;
}) {
    const five_hours_stale = (row.allowance?.read_seconds_ago ?? 0) >= FIVE_HOURS;
    const five_hours = five_hours_stale ? null : row.allowance?.session_percent;
    const out = out_words(row.allowance, now);
    const out_on_five_hours = out !== null && row.allowance?.limit_window === "session";

    return (
        <div className="flex flex-col gap-0.5">
            <div className="flex flex-wrap items-center gap-2">
                <Meter
                    percent={row.allowance?.weekly_percent}
                    point={week_at}
                    colour={row.allowance?.room === "spent" ? "spent" : nearness(row.allowance?.weekly_percent, week_at)}
                    says={week_words(row.allowance)}
                    of="a week"
                />
                <span className="font-mono text-[10px] text-shade">· {who_spends(row.agents)}</span>
            </div>
            <Meter
                percent={out_on_five_hours ? 100 : five_hours}
                point={five_hours_at}
                colour={out_on_five_hours ? "spent" : nearness(five_hours, five_hours_at)}
                says={five_hours_words(row.allowance)}
                of="five hours"
            />
            {out ? <span className="font-mono text-[10px] text-coral">{out}</span> : null}
        </div>
    );
}

/// A percentage a person types and the panel keeps: saved when they leave
/// the box or press Enter, and put back as it was when it is not a number.
function Point({ value, on_commit }: { value: number; on_commit: (percent: number) => unknown }) {
    const [draft, set_draft] = useState("");

    const commit = () => {
        const wanted = Math.round(Number(draft));
        set_draft("");
        if (draft.trim() && Number.isFinite(wanted) && wanted !== value) {
            void on_commit(wanted);
        }
    };

    return (
        <input
            type="number"
            min={50}
            max={99}
            className="w-14 rounded-md border border-reef bg-lagoon px-1.5 py-0.5 font-mono text-[11px]"
            value={draft === "" ? value : draft}
            onChange={(event) => set_draft(event.target.value)}
            onBlur={commit}
            onKeyDown={(event) => {
                if (event.key === "Enter") {
                    commit();
                }
            }}
        />
    );
}

/// The logins this machine holds.
///
/// A second subscription is a second config folder and nothing else. There is no
/// way to point a Max or Pro plan at a gateway — the plan is an OAuth login bound
/// to the provider's own endpoint, and routing it elsewhere spends API credit
/// instead. So an account here is a folder the engine signs into, and the crew
/// picks which folder each agent starts in.
///
/// Each login says how much of its week is gone and who is spending from it,
/// where it stands in the order they are used in, and the hand-overs are listed
/// under the switch: a list of logins with no numbers on it could not show the
/// one thing a second login is for.
export function AccountsSection({ on_open_pane }: { on_open_pane?: (session_id: string) => void }) {
    const [held, set_held] = useState<AccountsReport | null>(null);
    const [allowances, set_allowances] = useState<Allowance[]>([]);
    const [crew, set_crew] = useState<Agent[]>([]);
    const [journal, set_journal] = useState<JournalEntry[]>([]);
    const [engine, set_engine] = useState<string>("");
    const [label, set_label] = useState("");
    const [notice, set_notice] = useState<string | null>(null);
    /// The switch is a change on its way until the core answers; a second
    /// click while it is would flip it back before the first was heard.
    const [switching, set_switching] = useState(false);
    /// Enter in the name box adds as well as the button does, so the guard
    /// against adding the same login twice has to cover both.
    const adding = useRef(false);
    const [fallback_from, set_fallback_from] = useState("");
    const [fallback_to, set_fallback_to] = useState("");

    const refresh = useCallback(async () => {
        const [report, budget, agents, entries] = await Promise.all([
            list_accounts(),
            read_budget().catch(() => null),
            list_agents().catch(() => [] as Agent[]),
            read_journal({ kind: "accounts.handed_over", limit: 10 }).catch(() => [] as JournalEntry[]),
        ]);

        set_held(report);
        set_allowances(budget?.allowances ?? []);
        set_crew(agents);
        set_journal(entries);
        set_engine((current) => current || (report.engines[0]?.id ?? ""));
    }, []);

    useEffect(() => {
        refresh().catch((cause) => set_notice(String(cause)));
    }, [refresh]);

    const rows = useMemo(() => (held ? login_rows(held, allowances, crew) : []), [allowances, crew, held]);
    const handed = useMemo(() => hand_overs(journal), [journal]);
    const switch_at = held?.switch_at ?? SWITCH_AT_DEFAULT;
    const session_switch_at = held?.session_switch_at ?? SESSION_SWITCH_AT_DEFAULT;
    const fallbacks = held?.model_fallbacks ?? {};
    const now = Math.floor(Date.now() / 1000);

    const say = (cause: unknown) => set_notice(cause instanceof Error ? cause.message : String(cause));

    const rotate = useCallback((change: RotationChange) => {
        return set_account_rotation(change)
            .then((report) => {
                set_held(report);
                set_notice(null);
            })
            .catch(say);
    }, []);

    const add = useCallback(async () => {
        if (!engine || !label.trim() || adding.current) {
            return;
        }

        adding.current = true;
        try {
            const made = await add_account(engine, label);
            set_label("");
            set_notice(null);
            await refresh();

            const pane = await sign_in_account(made.engine_id, made.label);
            on_open_pane?.(pane.id);
        } catch (cause) {
            say(cause);
        } finally {
            adding.current = false;
        }
    }, [engine, label, on_open_pane, refresh]);

    const sign_in = useCallback(
        async (engine_id: string, account: string) => {
            try {
                const pane = await sign_in_account(engine_id, account);
                on_open_pane?.(pane.id);
                set_notice("The engine's own sign-in is open in a pane — finish it there.");
            } catch (cause) {
                say(cause);
            }
        },
        [on_open_pane],
    );

    const forget = useCallback(
        async (engine_id: string, account: string) => {
            try {
                set_held(await forget_account(engine_id, account));
                set_notice(null);
            } catch (cause) {
                say(cause);
            }
        },
        [],
    );

    if (!held) {
        return <Waiting says="asking the engines who they are…" className="font-mono text-[11px] text-shade" />;
    }

    return (
        <section className="flex flex-col gap-3">
            <p className="font-mono text-[11px] text-shade">
                One login per folder. Adding one makes an empty identity and opens the engine's own
                sign-in in a pane; whether it worked is read back from the engine, never assumed.
            </p>

            {held.engines.length === 0 ? (
                <p className="font-mono text-[11px] text-sun">
                    No engine on this machine keeps a config folder Agentland knows how to move, so
                    every pane spends from whoever you are already signed in as.
                </p>
            ) : null}

            <div className="flex flex-col gap-2">
                {rows.length === 0 ? (
                    <p className="font-mono text-[11px] text-shade">
                        Nothing here yet — the crew spends from whoever this machine is signed in as.
                    </p>
                ) : null}

                {rows.map((row) => {
                    const { rank, of } = rank_among(rows, row);

                    return (
                        <div
                            key={row.identity}
                            className={`flex flex-wrap items-center justify-between gap-2 rounded-lg border bg-lagoon-deep px-2 py-1.5 ${
                                row.allowance?.room === "spent" ? "border-coral/60" : "border-reef"
                            }`}
                        >
                            <div className="flex min-w-0 items-center gap-2">
                                {of > 1 ? (
                                    <div className="flex flex-col items-center">
                                        <Press
                                            className="font-mono text-[10px] leading-none text-shade hover:text-turquoise disabled:opacity-30"
                                            disabled={rank === 1}
                                            title="use this login before the one above it"
                                            busy_says=""
                                            on_press={() => rotate({ order: reorder(rows, row.identity, -1) })}
                                        >
                                            ▲
                                        </Press>
                                        <span
                                            className="my-0.5 font-mono text-[10px] text-driftwood"
                                            title="the order agents are started on, and moved on to"
                                        >
                                            {ordinal(rank)}
                                        </span>
                                        <Press
                                            className="font-mono text-[10px] leading-none text-shade hover:text-turquoise disabled:opacity-30"
                                            disabled={rank === of}
                                            title="use this login after the one below it"
                                            busy_says=""
                                            on_press={() => rotate({ order: reorder(rows, row.identity, 1) })}
                                        >
                                            ▼
                                        </Press>
                                    </div>
                                ) : null}

                                <div className="flex min-w-0 flex-col gap-0.5">
                                    <span className="font-mono text-[11px] text-linen">
                                        {row.engine_id} · {row.label ?? "this machine's own login"}
                                    </span>
                                    <span className="font-mono text-[10px] text-shade">
                                        {!row.account
                                            ? "signed in outside Agentland — where the crew spends until told otherwise"
                                            : !row.account.askable
                                              ? "this engine has no way to say who it is — open its pane to find out"
                                              : row.account.signed_in
                                                ? `${row.account.who ?? "signed in"}${row.account.plan ? ` · ${row.account.plan}` : ""}`
                                                : "the engine says nobody is signed in here — it is skipped until somebody is"}
                                    </span>
                                    <Allowances
                                        row={row}
                                        week_at={switch_at}
                                        five_hours_at={session_switch_at}
                                        now={now}
                                    />
                                </div>
                            </div>

                            {row.account ? (
                                <div className="flex items-center gap-2">
                                    <Press
                                        className="rounded-lg border border-reef px-2 py-1 font-mono text-[11px] text-shell hover:border-turquoise hover:text-turquoise"
                                        busy_says="opening…"
                                        on_press={() => sign_in(row.engine_id, row.label ?? "")}
                                    >
                                        {row.account.askable && row.account.signed_in ? "sign in again" : "sign in"}
                                    </Press>
                                    <Press
                                        className="rounded-lg border border-reef px-2 py-1 font-mono text-[11px] text-shell hover:border-coral hover:text-coral"
                                        busy_says="forgetting…"
                                        on_press={() => forget(row.engine_id, row.label ?? "")}
                                        title="The folder goes, and the credential in it goes with it."
                                    >
                                        forget
                                    </Press>
                                </div>
                            ) : null}
                        </div>
                    );
                })}
            </div>

            {held.engines.length > 0 ? (
                <div className="flex flex-wrap items-center gap-2">
                    <Picker
                        className="rounded-lg border border-reef bg-lagoon px-2 py-1 font-mono text-[11px]"
                        value={engine}
                        placeholder="which engine"
                        choices={held.engines.map((held_engine) => ({
                            value: held_engine.id,
                            label: held_engine.name,
                        }))}
                        on_pick={set_engine}
                    />

                    <input
                        className="min-w-[10rem] flex-1 rounded-lg border border-reef bg-lagoon px-2 py-1 font-mono text-[11px]"
                        placeholder="what you call this login, e.g. second"
                        value={label}
                        onChange={(event) => set_label(event.target.value)}
                        onKeyDown={(event) => {
                            if (event.key === "Enter") {
                                void add();
                            }
                        }}
                    />

                    <Press
                        className="rounded-lg border border-reef px-2 py-1 font-mono text-[11px] text-shell hover:border-turquoise hover:text-turquoise disabled:opacity-50"
                        disabled={!label.trim()}
                        busy_says="adding…"
                        on_press={add}
                    >
                        add and sign in
                    </Press>
                </div>
            ) : null}

            <div className="flex flex-col gap-2 rounded-lg border border-reef bg-lagoon-deep px-2 py-2">
                <label className="flex items-start gap-2">
                    <input
                        type="checkbox"
                        className="mt-0.5"
                        checked={held.failover}
                        disabled={switching}
                        onChange={(event) => {
                            set_switching(true);
                            set_account_failover(event.target.checked)
                                .then(set_held)
                                .catch(say)
                                .finally(() => set_switching(false));
                        }}
                    />
                    <span className="flex flex-col gap-0.5">
                        <span className="font-mono text-[11px] text-linen">Use the logins in turn</span>
                        <span className="font-mono text-[10px] text-shade">
                            A new agent starts on the first login in this order that has room. An agent
                            whose login passes either point below, or whose engine says it is out, is moved
                            to the next one once its pane is at rest, resumes the same conversation, and
                            is told to carry on. Off unless you say otherwise — it spends subscriptions
                            you may not have meant to spend this week.
                        </span>
                    </span>
                </label>

                <div className="ml-5 flex flex-wrap items-center gap-1.5 font-mono text-[11px] text-shell">
                    move an agent on at
                    <Point value={switch_at} on_commit={(percent) => rotate({ switch_at: percent })} />
                    % of a week, or at
                    <Point value={session_switch_at} on_commit={(percent) => rotate({ session_switch_at: percent })} />
                    % of its five hours
                    <span className="basis-full text-[10px] text-shade">
                        the marks on the bars; earlier leaves room to finish a turn on the old login
                    </span>
                </div>
            </div>

            <div className="flex flex-col gap-1.5 rounded-lg border border-reef bg-lagoon-deep px-2 py-2">
                <span className="font-mono text-[11px] text-linen">When a model's own limit runs out</span>
                <span className="font-mono text-[10px] text-shade">
                    Some models have a week of their own on top of the login's. An agent stopped on one is
                    carried to the next login in the order that still has it; where none does, it carries on
                    with the model named here, and goes back to its own once that limit comes round. With
                    nothing named, it waits for the reset and is told to carry on then.
                </span>
                {fallback_rows(fallbacks).map(({ from, to }) => (
                    <div key={from} className="flex items-center gap-2 font-mono text-[11px] text-shell">
                        <span className="text-linen">{from}</span>
                        <span className="text-shade">→</span>
                        <span className="text-linen">{to}</span>
                        <Press
                            className="rounded border border-reef px-1.5 text-[10px] text-shade hover:border-coral hover:text-coral"
                            title="wait for this model's reset instead"
                            busy_says=""
                            on_press={() => rotate({ model_fallbacks: without_fallback(fallbacks, from) })}
                        >
                            ×
                        </Press>
                    </div>
                ))}
                <div className="flex flex-wrap items-center gap-1.5 font-mono text-[11px] text-shell">
                    when
                    <input
                        className="w-24 rounded-md border border-reef bg-lagoon px-1.5 py-0.5 font-mono text-[11px]"
                        placeholder="fable"
                        value={fallback_from}
                        onChange={(event) => set_fallback_from(event.target.value)}
                    />
                    runs out, carry on with
                    <input
                        className="w-24 rounded-md border border-reef bg-lagoon px-1.5 py-0.5 font-mono text-[11px]"
                        placeholder="opus"
                        value={fallback_to}
                        onChange={(event) => set_fallback_to(event.target.value)}
                    />
                    <Press
                        className="rounded-md border border-reef px-2 py-0.5 text-[11px] text-shell hover:border-turquoise hover:text-turquoise disabled:opacity-50"
                        disabled={!fallback_from.trim() || !fallback_to.trim()}
                        busy_says="saving…"
                        on_press={() =>
                            rotate({ model_fallbacks: with_fallback(fallbacks, fallback_from, fallback_to) }).then(() => {
                                set_fallback_from("");
                                set_fallback_to("");
                            })
                        }
                    >
                        add
                    </Press>
                </div>
            </div>

            {handed.length > 0 ? (
                <div className="flex flex-col gap-1">
                    <span className="font-mono text-[10px] uppercase tracking-[0.14em] text-shade">
                        handed over lately
                    </span>
                    {handed.map((entry) => (
                        <div key={`${entry.at}-${entry.subject}`} className="flex gap-2 font-mono text-[11px]">
                            <span className="w-16 shrink-0 text-shade" title={exactly(entry.at)}>
                                {when(entry.at, now)}
                            </span>
                            <span className="text-driftwood">{entry.detail}</span>
                        </div>
                    ))}
                </div>
            ) : null}

            {notice ? <p className="font-mono text-[11px] text-sun">{notice}</p> : null}
        </section>
    );
}
