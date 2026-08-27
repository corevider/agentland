import { useCallback, useEffect, useState } from "react";

import { answer_approval, list_approvals, type Approval } from "@/lib/core";
import { use_services } from "@/workspace/registry";

export function ApprovalsPanel({ active }: { active: boolean }) {
    const { crew, open_session } = use_services();
    const [approvals, set_approvals] = useState<Approval[]>([]);
    const [notes, set_notes] = useState<Record<string, string>>({});
    const [notice, set_notice] = useState<string | null>(null);

    const refresh = useCallback(async () => {
        set_approvals(await list_approvals());
    }, []);

    useEffect(() => {
        if (!active) {
            return;
        }

        refresh().catch((cause) => set_notice(cause instanceof Error ? cause.message : String(cause)));
        const handle = window.setInterval(() => refresh().catch(() => undefined), 3000);
        return () => window.clearInterval(handle);
    }, [active, refresh]);

    const answer = useCallback(
        (approval: Approval, approved: boolean) => {
            set_notice(null);
            const note = notes[approval.id]?.trim();

            answer_approval(approval.id, approved, note || undefined)
                .then(() => {
                    set_notes((held) => {
                        const next = { ...held };
                        delete next[approval.id];
                        return next;
                    });
                    return refresh();
                })
                .catch((cause) => set_notice(cause instanceof Error ? cause.message : String(cause)));
        },
        [notes, refresh],
    );

    const waiting = approvals.filter((approval) => approval.verdict === "pending");
    const answered = approvals.filter((approval) => approval.verdict !== "pending");

    const asker = (id: string) => crew.find((agent) => agent.id === id) ?? null;

    return (
        <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-2 overflow-y-auto p-2.5">
            {notice ? (
                <div className="rounded-md border border-coral px-2 py-1 font-mono text-[11px] text-coral">
                    {notice}
                </div>
            ) : null}

            <section>
                <h3 className="mb-1 font-mono text-[9px] uppercase tracking-[0.14em] text-coral">
                    Waiting on you · {waiting.length}
                </h3>

                {waiting.length === 0 ? (
                    <p className="font-mono text-[10px] text-shade">
                        Nobody is blocked. An agent that needs a decision stops and asks here.
                    </p>
                ) : null}

                <div className="flex flex-col gap-1.5">
                    {waiting.map((approval) => {
                        const agent = asker(approval.requested_by);

                        return (
                            <article
                                key={approval.id}
                                className="rounded-md border border-coral/70 bg-lagoon-deep px-2 py-1.5"
                            >
                                <div className="text-[12px] text-linen">{approval.summary}</div>
                                {approval.detail ? (
                                    <pre className="mt-0.5 max-h-32 overflow-auto whitespace-pre-wrap font-mono text-[10px] text-driftwood">
                                        {approval.detail}
                                    </pre>
                                ) : null}

                                <div className="mt-1 flex flex-wrap items-center gap-1.5">
                                    <span className="font-mono text-[10px] text-shade">
                                        {agent ? agent.name : approval.requested_by} asked
                                    </span>
                                    {agent?.session_id ? (
                                        <button
                                            className="rounded border border-reef px-1.5 font-mono text-[10px] text-shell hover:border-foam"
                                            onClick={() => open_session(agent.session_id as string)}
                                        >
                                            see its terminal
                                        </button>
                                    ) : null}

                                    <input
                                        className="ml-auto min-w-[120px] flex-1 rounded-md border border-reef bg-lagoon font-mono text-[10px]"
                                        placeholder="a note back (optional)"
                                        value={notes[approval.id] ?? ""}
                                        onChange={(event) =>
                                            set_notes((held) => ({
                                                ...held,
                                                [approval.id]: event.target.value,
                                            }))
                                        }
                                    />
                                    <button
                                        className="rounded border border-palm px-2 font-mono text-[11px] text-palm"
                                        onClick={() => answer(approval, true)}
                                    >
                                        approve
                                    </button>
                                    <button
                                        className="rounded border border-coral px-2 font-mono text-[11px] text-coral"
                                        onClick={() => answer(approval, false)}
                                    >
                                        reject
                                    </button>
                                </div>
                            </article>
                        );
                    })}
                </div>
            </section>

            <section className="min-h-0">
                <h3 className="mb-1 font-mono text-[9px] uppercase tracking-[0.14em] text-shade">
                    Already answered · {answered.length}
                </h3>

                {answered.length === 0 ? (
                    <p className="font-mono text-[10px] text-shade">Nothing answered yet.</p>
                ) : null}

                <div className="flex flex-col gap-1">
                    {[...answered].reverse().map((approval) => (
                        <article
                            key={approval.id}
                            className="rounded-md border border-reef bg-lagoon-deep px-2 py-1"
                        >
                            <div className="flex items-baseline gap-2">
                                <span className="truncate text-[12px] text-linen">{approval.summary}</span>
                                <span
                                    className={`ml-auto font-mono text-[10px] ${
                                        approval.verdict === "approved" ? "text-palm" : "text-coral"
                                    }`}
                                >
                                    {approval.verdict}
                                </span>
                            </div>
                            <div className="mt-0.5 font-mono text-[10px] text-shade">
                                {asker(approval.requested_by)?.name ?? approval.requested_by}
                                {approval.answered_note ? ` · you said: ${approval.answered_note}` : ""}
                            </div>
                        </article>
                    ))}
                </div>
            </section>
        </div>
    );
}
