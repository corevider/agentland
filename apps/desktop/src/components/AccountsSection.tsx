import { useCallback, useEffect, useMemo, useState } from "react";

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
} from "@/lib/core";
import {
    hand_overs,
    login_rows,
    ordinal,
    rank_among,
    reorder,
    week_words,
    who_spends,
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

/// How much of a login's week is gone, as a bar with the words beside it.
function Week({ row, switch_at }: { row: LoginRow; switch_at: number }) {
    const spent = row.allowance?.weekly_percent;
    const known = spent !== undefined && spent !== null;

    return (
        <div className="flex flex-wrap items-center gap-2">
            <span className="relative h-1.5 w-28 overflow-hidden rounded-full bg-reef/60">
                {known ? (
                    <span
                        className={`block h-full ${ROOM_COLOUR[row.allowance?.room ?? "plenty"]}`}
                        style={{ width: `${Math.min(100, Math.max(0, spent))}%` }}
                    />
                ) : null}
                <span
                    className="absolute top-0 h-full w-px bg-linen/70"
                    style={{ left: `${switch_at}%` }}
                    title={`agents are moved on at ${switch_at}%`}
                />
            </span>
            <span className="font-mono text-[10px] text-shell">{week_words(row.allowance)}</span>
            <span className="font-mono text-[10px] text-shade">· {who_spends(row.agents)}</span>
        </div>
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
    const [switch_draft, set_switch_draft] = useState<string>("");
    const [notice, set_notice] = useState<string | null>(null);
    const [busy, set_busy] = useState(false);

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
    const now = Math.floor(Date.now() / 1000);

    const say = (cause: unknown) => set_notice(cause instanceof Error ? cause.message : String(cause));

    const rotate = useCallback((change: { order?: string[]; switch_at?: number }) => {
        set_account_rotation(change)
            .then((report) => {
                set_held(report);
                set_notice(null);
            })
            .catch(say);
    }, []);

    const commit_switch = useCallback(() => {
        const wanted = Number(switch_draft);
        set_switch_draft("");
        if (!switch_draft.trim() || !Number.isFinite(wanted) || wanted === switch_at) {
            return;
        }
        rotate({ switch_at: Math.round(wanted) });
    }, [rotate, switch_at, switch_draft]);

    const add = useCallback(async () => {
        if (!engine || !label.trim()) {
            return;
        }

        set_busy(true);
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
            set_busy(false);
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
                                        <button
                                            className="font-mono text-[10px] leading-none text-shade hover:text-turquoise disabled:opacity-30"
                                            disabled={rank === 1}
                                            title="use this login before the one above it"
                                            onClick={() => rotate({ order: reorder(rows, row.identity, -1) })}
                                        >
                                            ▲
                                        </button>
                                        <span
                                            className="my-0.5 font-mono text-[10px] text-driftwood"
                                            title="the order agents are started on, and moved on to"
                                        >
                                            {ordinal(rank)}
                                        </span>
                                        <button
                                            className="font-mono text-[10px] leading-none text-shade hover:text-turquoise disabled:opacity-30"
                                            disabled={rank === of}
                                            title="use this login after the one below it"
                                            onClick={() => rotate({ order: reorder(rows, row.identity, 1) })}
                                        >
                                            ▼
                                        </button>
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
                                    <Week row={row} switch_at={switch_at} />
                                </div>
                            </div>

                            {row.account ? (
                                <div className="flex items-center gap-2">
                                    <button
                                        className="rounded-lg border border-reef px-2 py-1 font-mono text-[11px] text-shell hover:border-turquoise hover:text-turquoise"
                                        onClick={() => void sign_in(row.engine_id, row.label ?? "")}
                                    >
                                        {row.account.askable && row.account.signed_in ? "sign in again" : "sign in"}
                                    </button>
                                    <button
                                        className="rounded-lg border border-reef px-2 py-1 font-mono text-[11px] text-shell hover:border-coral hover:text-coral"
                                        onClick={() => void forget(row.engine_id, row.label ?? "")}
                                        title="The folder goes, and the credential in it goes with it."
                                    >
                                        forget
                                    </button>
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

                    <button
                        className="rounded-lg border border-reef px-2 py-1 font-mono text-[11px] text-shell hover:border-turquoise hover:text-turquoise disabled:opacity-50"
                        disabled={busy || !label.trim()}
                        onClick={() => void add()}
                    >
                        add and sign in
                    </button>
                </div>
            ) : null}

            <div className="flex flex-col gap-2 rounded-lg border border-reef bg-lagoon-deep px-2 py-2">
                <label className="flex items-start gap-2">
                    <input
                        type="checkbox"
                        className="mt-0.5"
                        checked={held.failover}
                        onChange={(event) => {
                            set_account_failover(event.target.checked).then(set_held).catch(say);
                        }}
                    />
                    <span className="flex flex-col gap-0.5">
                        <span className="font-mono text-[11px] text-linen">Use the logins in turn</span>
                        <span className="font-mono text-[10px] text-shade">
                            A new agent starts on the first login in this order that has room. An agent
                            whose login passes the point below, or whose engine says it is out, is moved
                            to the next one once its pane is at rest, resumes the same conversation, and
                            is told to carry on. Off unless you say otherwise — it spends subscriptions
                            you may not have meant to spend this week.
                        </span>
                    </span>
                </label>

                <label className="ml-5 flex flex-wrap items-center gap-1.5 font-mono text-[11px] text-shell">
                    move an agent on at
                    <input
                        type="number"
                        min={50}
                        max={99}
                        className="w-14 rounded-md border border-reef bg-lagoon px-1.5 py-0.5 font-mono text-[11px]"
                        value={switch_draft === "" ? switch_at : switch_draft}
                        onChange={(event) => set_switch_draft(event.target.value)}
                        onBlur={commit_switch}
                        onKeyDown={(event) => {
                            if (event.key === "Enter") {
                                commit_switch();
                            }
                        }}
                    />
                    % of a week
                    <span className="text-[10px] text-shade">
                        — the mark on each bar; earlier leaves room to finish a turn on the old login
                    </span>
                </label>
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
