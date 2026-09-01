import { useCallback, useEffect, useState } from "react";
import { motion } from "motion/react";

import {
    answer_approval,
    assign_task,
    create_task,
    list_approvals,
    read_log,
    start_agent,
    stop_agent,
    type Agent,
    type Approval,
} from "@/lib/core";
import { PRESENCE_COLOR, PRESENCE_LABEL } from "@/island/geometry";

interface Props {
    agent: Agent;
    on_close: () => void;
    on_open_pane: (session_id: string) => void;
    on_changed: () => void;
}

export function AgentSheet({ agent, on_close, on_open_pane, on_changed }: Props) {
    const [tail, set_tail] = useState<string>("");
    const [approvals, set_approvals] = useState<Approval[]>([]);
    const [instruction, set_instruction] = useState("");
    const [busy, set_busy] = useState(false);
    const [notice, set_notice] = useState<string | null>(null);

    const refresh = useCallback(async () => {
        const pending = await list_approvals();
        set_approvals(
            pending.filter((entry) => entry.verdict === "pending" && entry.requested_by === agent.id),
        );

        if (agent.session_id) {
            const log = await read_log(agent.session_id, 2400).catch(() => "");
            const cleaned = log
                // eslint-disable-next-line no-control-regex
                .replace(/\[[0-9;?]*[a-zA-Z]/g, "")
                .replace(/\r/g, "")
                .split("\n")
                .filter((line) => line.trim().length > 0)
                .slice(-14)
                .join("\n");
            set_tail(cleaned);
        } else {
            set_tail("");
        }
    }, [agent.id, agent.session_id]);

    useEffect(() => {
        refresh().catch(() => undefined);
        const handle = window.setInterval(() => refresh().catch(() => undefined), 3000);
        return () => window.clearInterval(handle);
    }, [refresh]);

    const run = useCallback(
        async (action: () => Promise<unknown>, done?: string) => {
            set_busy(true);
            set_notice(null);
            try {
                await action();
                if (done) {
                    set_notice(done);
                }
                on_changed();
                await refresh();
            } catch (cause) {
                set_notice(cause instanceof Error ? cause.message : String(cause));
            } finally {
                set_busy(false);
            }
        },
        [on_changed, refresh],
    );

    const give_work = useCallback(() => {
        const title = instruction.trim();
        if (!title) {
            return;
        }

        void run(async () => {
            const task = await create_task(title, "", agent.repository_id);
            await assign_task(task.id, agent.id);
            set_instruction("");
        }, `${agent.name} took it on`);
    }, [agent.id, agent.name, agent.repository_id, instruction, run]);

    return (
        <motion.aside
            initial={{ x: 24, opacity: 0 }}
            animate={{ x: 0, opacity: 1 }}
            transition={{ type: "spring", stiffness: 460, damping: 40 }}
            data-overlay
            onClick={(event) => event.stopPropagation()}
            onPointerDown={(event) => event.stopPropagation()}
            className="absolute inset-y-0 right-0 z-10 flex w-full max-w-[380px] flex-col border-l border-reef bg-lagoon/95 backdrop-blur">
            <header className="flex items-start justify-between gap-3 border-b border-reef px-2.5 py-1.5">
                <div>
                    <div className="font-display text-[15px] text-linen">{agent.name}</div>
                    <div className="font-mono text-[11px] text-shell">
                        {agent.role} · {agent.engine_id} · {agent.repository_id}/{agent.worktree}
                    </div>
                    <div
                        className="mt-1 font-mono text-[11px]"
                        style={{ color: PRESENCE_COLOR[agent.presence] ?? PRESENCE_COLOR.idle }}
                        title={agent.reason}
                    >
                        {PRESENCE_LABEL[agent.presence] ?? agent.presence} — {agent.reason}
                    </div>
                </div>
                <button className="rounded-lg border border-foam px-2 py-1 font-mono text-[11px]" onClick={on_close}>
                    close
                </button>
            </header>

            <div className="flex min-h-0 flex-1 flex-col gap-2.5 overflow-y-auto p-2.5">
                {approvals.length > 0 ? (
                    <section className="rounded-lg border border-coral p-2">
                        <h3 className="mb-1 font-mono text-[10px] uppercase tracking-[0.12em] text-coral">
                            Waiting on you
                        </h3>
                        {approvals.map((approval) => (
                            <div key={approval.id} className="mb-2 last:mb-0">
                                <div className="text-[13px] text-linen">{approval.summary}</div>
                                {approval.detail ? (
                                    <div className="mt-1 font-mono text-[11px] text-shell">{approval.detail}</div>
                                ) : null}
                                <div className="mt-2 flex gap-2">
                                    <button
                                        className="flex-1 rounded-lg border border-palm px-2 py-1 text-[11px] text-palm disabled:opacity-40"
                                        disabled={busy}
                                        onClick={() => run(() => answer_approval(approval.id, true), "Approved")}
                                    >
                                        Approve
                                    </button>
                                    <button
                                        className="flex-1 rounded-lg border border-coral px-2 py-1 text-[11px] text-coral disabled:opacity-40"
                                        disabled={busy}
                                        onClick={() => run(() => answer_approval(approval.id, false), "Rejected")}
                                    >
                                        Reject
                                    </button>
                                </div>
                            </div>
                        ))}
                    </section>
                ) : null}

                <section>
                    <h3 className="mb-1 font-mono text-[10px] uppercase tracking-[0.12em] text-shell">
                        Give it work
                    </h3>
                    <div className="flex gap-2">
                        <input
                            className="min-w-0 flex-1 rounded-lg border border-reef bg-lagoon-deep px-2 py-1 text-[11px]"
                            placeholder="tighten the subscription guard"
                            value={instruction}
                            onChange={(event) => set_instruction(event.target.value)}
                            onKeyDown={(event) => {
                                if (event.key === "Enter") {
                                    give_work();
                                }
                            }}
                        />
                        <button
                            className="rounded-lg border border-turquoise px-3 py-1 text-[11px] text-turquoise disabled:opacity-40"
                            disabled={busy || instruction.trim().length === 0}
                            onClick={give_work}
                        >
                            send
                        </button>
                    </div>
                    <p className="mt-1 font-mono text-[10px] text-shade">
                        Becomes a card, then its opening prompt.
                    </p>
                </section>

                <section className="flex flex-wrap gap-2">
                    {agent.session_id ? (
                        <>
                            <button
                                className="rounded-lg border border-foam px-3 py-1 text-[11px]"
                                onClick={() => on_open_pane(agent.session_id as string)}
                            >
                                open its terminal
                            </button>
                            <button
                                className="rounded-lg border border-foam px-3 py-1 text-[11px] disabled:opacity-40"
                                disabled={busy}
                                onClick={() => run(() => stop_agent(agent.id), "Stopped")}
                            >
                                stop
                            </button>
                        </>
                    ) : (
                        <>
                            <button
                                className="rounded-lg border border-foam px-3 py-1 text-[11px] disabled:opacity-40"
                                disabled={busy}
                                onClick={() => run(() => start_agent(agent.id, false), "Started")}
                            >
                                start
                            </button>
                            <button
                                className="rounded-lg border border-foam px-3 py-1 text-[11px] disabled:opacity-40"
                                disabled={busy}
                                onClick={() => run(() => start_agent(agent.id, true), "Resumed")}
                            >
                                resume
                            </button>
                        </>
                    )}
                </section>

                {notice ? <div className="font-mono text-[11px] text-driftwood">{notice}</div> : null}

                <section className="shrink-0">
                    <h3 className="mb-1 font-mono text-[10px] uppercase tracking-[0.12em] text-shell">
                        Last words
                    </h3>
                    <pre className="max-h-72 overflow-auto rounded-lg border border-reef bg-lagoon-deep p-2 font-mono text-[10px] leading-relaxed text-driftwood">
                        {tail || "It has not said anything yet."}
                    </pre>
                </section>
            </div>
        </motion.aside>
    );
}
