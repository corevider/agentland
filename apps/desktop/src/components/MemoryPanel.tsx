import { useCallback, useEffect, useState } from "react";

import {
    answer_memory,
    forget_memory,
    list_memories,
    propose_memory,
    type Memory,
    type MemoryScope,
} from "@/lib/core";
import { use_services } from "@/workspace/registry";

const SCOPES: MemoryScope[] = ["workspace", "repository", "agent"];

export function MemoryPanel({ active }: { active: boolean }) {
    const { crew } = use_services();
    const [memories, set_memories] = useState<Memory[]>([]);
    const [draft, set_draft] = useState<{ text: string; scope: MemoryScope; scope_id: string }>({
        text: "",
        scope: "workspace",
        scope_id: "",
    });
    const [notice, set_notice] = useState<string | null>(null);

    const refresh = useCallback(async () => {
        set_memories(await list_memories());
    }, []);

    useEffect(() => {
        if (!active) {
            return;
        }

        refresh().catch((cause) => set_notice(cause instanceof Error ? cause.message : String(cause)));
        const handle = window.setInterval(() => refresh().catch(() => undefined), 5000);
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

    const propose = useCallback(() => {
        const text = draft.text.trim();
        if (!text) {
            return;
        }

        run(async () => {
            await propose_memory(text, draft.scope, draft.scope_id, "you");
            set_draft({ ...draft, text: "" });
        });
    }, [draft, run]);

    const waiting = memories.filter((memory) => !memory.approved);
    const kept = memories.filter((memory) => memory.approved);

    return (
        <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-2 overflow-y-auto p-2.5">
            <section className="flex flex-wrap items-center gap-1.5">
                <input
                    className="min-w-[150px] flex-1 rounded-md border border-reef bg-lagoon-deep font-mono text-[11px]"
                    placeholder="the dev server needs PORT, not --port"
                    value={draft.text}
                    onChange={(event) => set_draft({ ...draft, text: event.target.value })}
                    onKeyDown={(event) => event.key === "Enter" && propose()}
                />
                <select
                    className="rounded-md border border-reef bg-lagoon-deep font-mono text-[11px]"
                    value={draft.scope}
                    onChange={(event) =>
                        set_draft({ ...draft, scope: event.target.value as MemoryScope })
                    }
                >
                    {SCOPES.map((scope) => (
                        <option key={scope} value={scope}>
                            {scope}
                        </option>
                    ))}
                </select>
                {draft.scope === "workspace" ? null : (
                    <input
                        className="w-32 rounded-md border border-reef bg-lagoon-deep font-mono text-[11px]"
                        placeholder={draft.scope === "agent" ? "agent id" : "repository id"}
                        value={draft.scope_id}
                        onChange={(event) => set_draft({ ...draft, scope_id: event.target.value })}
                    />
                )}
                <button
                    className="rounded-md border border-turquoise px-2 py-0.5 font-mono text-[11px] text-turquoise"
                    onClick={propose}
                >
                    propose
                </button>
            </section>

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
                        Nothing proposed. An agent's memory only reaches the crew once you approve it.
                    </p>
                ) : null}
                <div className="flex flex-col gap-1">
                    {waiting.map((memory) => (
                        <Entry
                            key={memory.id}
                            memory={memory}
                            on_approve={() => run(() => answer_memory(memory.id, true))}
                            on_forget={() => run(() => forget_memory(memory.id))}
                        />
                    ))}
                </div>
            </section>

            <section className="min-h-0">
                <h3 className="mb-1 font-mono text-[9px] uppercase tracking-[0.14em] text-shade">
                    What the crew has learned · {kept.length}
                </h3>
                {kept.length === 0 ? (
                    <p className="font-mono text-[10px] text-shade">Nothing approved yet.</p>
                ) : null}
                <div className="flex flex-col gap-1">
                    {kept.map((memory) => (
                        <Entry
                            key={memory.id}
                            memory={memory}
                            on_revoke={() => run(() => answer_memory(memory.id, false))}
                            on_forget={() => run(() => forget_memory(memory.id))}
                        />
                    ))}
                </div>
            </section>
        </div>
    );
}

function Entry({
    memory,
    on_approve,
    on_revoke,
    on_forget,
}: {
    memory: Memory;
    on_approve?: () => void;
    on_revoke?: () => void;
    on_forget: () => void;
}) {
    return (
        <article
            className={`rounded-md border bg-lagoon-deep px-2 py-1 ${
                memory.approved ? "border-reef" : "border-coral/60"
            }`}
        >
            <div className="text-[12px] text-linen">{memory.text}</div>
            <div className="mt-0.5 flex flex-wrap items-center gap-2 font-mono text-[10px] text-shade">
                <span>{memory.scope}{memory.scope_id ? ` · ${memory.scope_id}` : ""}</span>
                <span>from {memory.proposed_by}</span>
                {memory.masked ? (
                    <span className="text-sun" title="a secret was masked before this was stored">
                        masked
                    </span>
                ) : null}
                <span className="ml-auto flex gap-1">
                    {on_approve ? (
                        <button
                            className="rounded border border-palm px-1.5 text-palm"
                            onClick={on_approve}
                        >
                            approve
                        </button>
                    ) : null}
                    {on_revoke ? (
                        <button
                            className="rounded border border-reef px-1.5 hover:border-sun hover:text-sun"
                            title="take it back out of the crew's brief without deleting it"
                            onClick={on_revoke}
                        >
                            revoke
                        </button>
                    ) : null}
                    <button className="rounded border border-reef px-1.5 hover:border-coral hover:text-coral" onClick={on_forget}>
                        forget
                    </button>
                </span>
            </div>
        </article>
    );
}
