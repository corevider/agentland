import { use_poll } from "@/lib/poll";

import { exactly, when } from "@/lib/when";
import { useCallback, useEffect, useMemo, useState } from "react";

import {
    list_mail,
    mail_policy,
    send_mail,
    set_mail_policy,
    type MailMessage,
    type MailPolicy,
} from "@/lib/core";
import { use_services } from "@/workspace/registry";

export function MailPanel({ active }: { active: boolean }) {
    const { crew } = use_services();
    const [messages, set_messages] = useState<MailMessage[]>([]);
    const [policy, set_policy] = useState<MailPolicy | null>(null);
    const [draft, set_draft] = useState({ from: "", to: "", text: "" });
    const [notice, set_notice] = useState<string | null>(null);

    const refresh = useCallback(async () => {
        const [held, rules] = await Promise.all([list_mail(), mail_policy()]);
        set_messages(held);
        set_policy(rules);
    }, []);

    use_poll(() => {
        refresh().catch((cause) => set_notice(cause instanceof Error ? cause.message : String(cause)));
    }, 4000, active);

    const names = useMemo(() => crew.map((agent) => agent.id), [crew]);

    const run = useCallback(
        (action: () => Promise<unknown>) => {
            set_notice(null);
            action()
                .then(() => refresh())
                .catch((cause) => set_notice(cause instanceof Error ? cause.message : String(cause)));
        },
        [refresh],
    );

    const send = useCallback(() => {
        const text = draft.text.trim();
        const from = draft.from || names[0];
        const to = draft.to || names[1] || names[0];

        if (!text || !from || !to) {
            set_notice("a message needs a sender, a reader and something to say");
            return;
        }

        run(async () => {
            await send_mail(from, to, text);
            set_draft({ ...draft, text: "" });
        });
    }, [draft, names, run]);

    const waiting = messages.filter((message) => !message.delivered);

    return (
        <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-2 overflow-y-auto p-2.5">
            <section className="flex flex-wrap items-center gap-2">
                <span
                    className={`rounded-md border px-2 py-0.5 font-mono text-[11px] ${
                        policy?.paused ? "border-coral text-coral" : "border-palm text-palm"
                    }`}
                >
                    {policy?.paused ? "mail is paused" : "mail is flowing"}
                </span>

                <button
                    className="rounded-md border border-foam px-2 py-0.5 font-mono text-[11px]"
                    disabled={!policy}
                    onClick={() => policy && run(() => set_mail_policy({ ...policy, paused: !policy.paused }))}
                >
                    {policy?.paused ? "let it flow" : "pause everything"}
                </button>

                <button
                    className="rounded-md border border-reef px-2 py-0.5 font-mono text-[11px] text-shell hover:border-foam"
                    disabled={!policy}
                    onClick={() =>
                        policy &&
                        run(() => set_mail_policy({ ...policy, allow_unlisted: !policy.allow_unlisted }))
                    }
                    title="whether an agent may write to someone it has no grant for"
                >
                    unlisted pairs: {policy?.allow_unlisted ? "allowed" : "refused"}
                </button>

                <span className="font-mono text-[10px] text-shade">
                    {waiting.length} waiting · {messages.length} in the record
                </span>
            </section>

            {notice ? (
                <div className="rounded-md border border-coral px-2 py-1 font-mono text-[11px] text-coral">
                    {notice}
                </div>
            ) : null}

            <section className="flex flex-wrap items-center gap-1.5">
                <select
                    className="rounded-md border border-reef bg-lagoon-deep font-mono text-[11px]"
                    value={draft.from || names[0] || ""}
                    onChange={(event) => set_draft({ ...draft, from: event.target.value })}
                >
                    {names.map((name) => (
                        <option key={name} value={name}>
                            {name}
                        </option>
                    ))}
                </select>
                <span className="font-mono text-[11px] text-shade">→</span>
                <select
                    className="rounded-md border border-reef bg-lagoon-deep font-mono text-[11px]"
                    value={draft.to || names[1] || names[0] || ""}
                    onChange={(event) => set_draft({ ...draft, to: event.target.value })}
                >
                    {names.map((name) => (
                        <option key={name} value={name}>
                            {name}
                        </option>
                    ))}
                </select>
                <input
                    className="min-w-[140px] flex-1 rounded-md border border-reef bg-lagoon-deep font-mono text-[11px]"
                    placeholder="the port is 4103, not 3000"
                    value={draft.text}
                    onChange={(event) => set_draft({ ...draft, text: event.target.value })}
                    onKeyDown={(event) => event.key === "Enter" && send()}
                />
                <button
                    className="rounded-md border border-turquoise px-2 py-0.5 font-mono text-[11px] text-turquoise"
                    onClick={send}
                >
                    send
                </button>
            </section>

            <section className="min-h-0">
                <h3 className="mb-1 font-mono text-[9px] uppercase tracking-[0.14em] text-shade">
                    The record
                </h3>

                {messages.length === 0 ? (
                    <p className="font-mono text-[10px] text-shade">
                        Nothing has been sent. A message reaches its reader in the opening brief the next
                        time that agent starts.
                    </p>
                ) : null}

                <div className="flex flex-col gap-1">
                    {[...messages].reverse().map((message) => (
                        <article
                            key={message.id}
                            className="rounded-md border border-reef bg-lagoon-deep px-2 py-1"
                        >
                            <div className="flex items-baseline gap-2 font-mono text-[10px]">
                                <span className="text-linen">{message.from}</span>
                                <span className="text-shade">→</span>
                                <span className="text-linen">{message.to}</span>
                                <span className="ml-auto text-shade" title={exactly(message.at ?? 0)}>
                                    {when(message.at ?? 0, Math.floor(Date.now() / 1000))}
                                </span>
                                <span className={message.delivered ? "text-shade" : "text-sun"}>
                                    {message.delivered ? "read" : "waiting"}
                                </span>
                            </div>
                            <div className="mt-0.5 text-[12px] text-driftwood">{message.text}</div>
                        </article>
                    ))}
                </div>
            </section>
        </div>
    );
}
