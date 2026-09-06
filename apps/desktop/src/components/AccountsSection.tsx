import { useCallback, useEffect, useState } from "react";

import { Waiting } from "@/components/Spinner";
import {
    add_account,
    forget_account,
    list_accounts,
    set_account_failover,
    sign_in_account,
    type AccountsReport,
} from "@/lib/core";

/// The logins this machine holds.
///
/// A second subscription is a second config folder and nothing else. There is no
/// way to point a Max or Pro plan at a gateway — the plan is an OAuth login bound
/// to the provider's own endpoint, and routing it elsewhere spends API credit
/// instead. So an account here is a folder the engine signs into, and the crew
/// picks which folder each agent starts in.
export function AccountsSection({ on_open_pane }: { on_open_pane?: (session_id: string) => void }) {
    const [held, set_held] = useState<AccountsReport | null>(null);
    const [engine, set_engine] = useState<string>("");
    const [label, set_label] = useState("");
    const [notice, set_notice] = useState<string | null>(null);
    const [busy, set_busy] = useState(false);

    const refresh = useCallback(async () => {
        const report = await list_accounts();
        set_held(report);
        set_engine((current) => current || (report.engines[0]?.id ?? ""));
    }, []);

    useEffect(() => {
        refresh().catch((cause) => set_notice(String(cause)));
    }, [refresh]);

    const say = (cause: unknown) => set_notice(cause instanceof Error ? cause.message : String(cause));

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
                {held.accounts.length === 0 ? (
                    <p className="font-mono text-[11px] text-shade">
                        Nothing here yet — the crew spends from whoever this machine is signed in as.
                    </p>
                ) : null}

                {held.accounts.map((account) => (
                    <div
                        key={`${account.engine_id}/${account.label}`}
                        className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-reef bg-lagoon-deep px-2 py-1.5"
                    >
                        <div className="flex flex-col">
                            <span className="font-mono text-[11px] text-linen">
                                {account.engine_id} · {account.label}
                            </span>
                            <span className="font-mono text-[10px] text-shade">
                                {!account.askable
                                    ? "this engine has no way to say who it is — open its pane to find out"
                                    : account.signed_in
                                      ? `${account.who ?? "signed in"}${account.plan ? ` · ${account.plan}` : ""}`
                                      : "the engine says nobody is signed in here"}
                            </span>
                        </div>

                        <div className="flex items-center gap-2">
                            <button
                                className="rounded-lg border border-reef px-2 py-1 font-mono text-[11px] text-shell hover:border-turquoise hover:text-turquoise"
                                onClick={() => void sign_in(account.engine_id, account.label)}
                            >
                                {account.askable && account.signed_in ? "sign in again" : "sign in"}
                            </button>
                            <button
                                className="rounded-lg border border-reef px-2 py-1 font-mono text-[11px] text-shell hover:border-coral hover:text-coral"
                                onClick={() => void forget(account.engine_id, account.label)}
                                title="The folder goes, and the credential in it goes with it."
                            >
                                forget
                            </button>
                        </div>
                    </div>
                ))}
            </div>

            {held.engines.length > 0 ? (
                <div className="flex flex-wrap items-center gap-2">
                    <select
                        className="rounded-lg border border-reef bg-lagoon px-2 py-1 font-mono text-[11px]"
                        value={engine}
                        onChange={(event) => set_engine(event.target.value)}
                    >
                        {held.engines.map((held_engine) => (
                            <option key={held_engine.id} value={held_engine.id}>
                                {held_engine.name}
                            </option>
                        ))}
                    </select>

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

            <label className="flex items-start gap-2 rounded-lg border border-reef bg-lagoon-deep px-2 py-2">
                <input
                    type="checkbox"
                    className="mt-0.5"
                    checked={held.failover}
                    onChange={(event) => {
                        set_account_failover(event.target.checked).then(set_held).catch(say);
                    }}
                />
                <span className="flex flex-col gap-0.5">
                    <span className="font-mono text-[11px] text-linen">
                        Carry on with the other login when a week runs out
                    </span>
                    <span className="font-mono text-[10px] text-shade">
                        Off unless you say otherwise — it spends a second subscription you did not
                        ask to spend this week. A pane already running keeps the login it started
                        with; the hand-over trades it for a fresh one and resumes the same
                        conversation.
                    </span>
                </span>
            </label>

            {notice ? <p className="font-mono text-[11px] text-sun">{notice}</p> : null}
        </section>
    );
}
