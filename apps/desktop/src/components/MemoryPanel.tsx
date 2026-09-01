import { use_poll } from "@/lib/poll";
import { useCallback, useEffect, useState } from "react";

import {
    answer_memory,
    forget_memory,
    list_memories,
    propose_memory,
    read_embedder,
    search_memories,
    set_embedder,
    type EmbedderReport,
    type Memory,
    type MemoryScope,
    type Recalled,
} from "@/lib/core";
import { use_services } from "@/workspace/registry";
import { exactly, when } from "@/lib/when";

/// Where a memory can be filed, in the vault's own words. A scope is also told
/// everything above it, so the crew's own shelf reaches every project.
const SCOPES: { value: MemoryScope; label: string }[] = [
    { value: "shared", label: "the whole crew" },
    { value: "workspace", label: "this workspace" },
    { value: "project", label: "one project" },
];

export function MemoryPanel({ active }: { active: boolean }) {
    const { crew } = use_services();
    const [memories, set_memories] = useState<Memory[]>([]);
    const [draft, set_draft] = useState<{ text: string; scope: MemoryScope; project: string }>({
        text: "",
        scope: "shared",
        project: "",
    });
    const [notice, set_notice] = useState<string | null>(null);
    const [query, set_query] = useState("");
    const [found, set_found] = useState<Recalled[] | null>(null);
    const [embedder, set_embedder_report] = useState<EmbedderReport | null>(null);
    const [tuning, set_tuning] = useState(false);

    const refresh = useCallback(async () => {
        const [held, report] = await Promise.all([list_memories(), read_embedder()]);
        set_memories(held);
        set_embedder_report(report);
    }, []);

    const search = useCallback(() => {
        const text = query.trim();
        if (!text) {
            set_found(null);
            return;
        }

        search_memories(text)
            .then(set_found)
            .catch((cause) => set_notice(cause instanceof Error ? cause.message : String(cause)));
    }, [query]);

    use_poll(() => {
        refresh().catch(() => undefined);
    }, 5000, active);

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

        // "project" needs the project's name to become a folder; the workspace
        // half is filled in by the core, which knows which one is active.
        const scope =
            draft.scope === "project" && draft.project.trim()
                ? `project:${draft.project.trim()}`
                : draft.scope === "workspace"
                  ? "workspace"
                  : draft.scope;

        run(async () => {
            await propose_memory(text, scope, "you");
            set_draft({ ...draft, text: "" });
        });
    }, [draft, run]);

    // Newest first, in both lists: a correction is written after the thing it
    // corrects, so the one that matters is the one at the top.
    const newest_first = [...memories].sort((left, right) => right.written_at - left.written_at);
    // Three states, not two: something nobody has answered is a question, and
    // something a person took back out is an answer. Showing them in one list
    // asks the same question twice, which is how a memory that was deliberately
    // retired ends up re-approved by someone tidying the top of the panel.
    const waiting = newest_first.filter((memory) => !memory.approved && !memory.retired);
    const kept = newest_first.filter((memory) => memory.approved);
    const retired = newest_first.filter((memory) => !memory.approved && memory.retired);
    const by_slug = new Map(memories.map((memory) => [memory.id, memory]));

    return (
        <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-2 overflow-y-auto p-2.5">
            <section className="flex shrink-0 flex-wrap items-center gap-1.5">
                <input
                    className="min-w-[150px] flex-1 rounded-md border border-reef bg-lagoon-deep font-mono text-[11px]"
                    placeholder="what would an agent be told about ports?"
                    value={query}
                    onChange={(event) => set_query(event.target.value)}
                    onKeyDown={(event) => event.key === "Enter" && search()}
                />
                <button
                    className="rounded-md border border-foam px-2 py-0.5 font-mono text-[11px]"
                    onClick={search}
                >
                    recall
                </button>
                {found ? (
                    <button
                        className="rounded-md border border-reef px-2 py-0.5 font-mono text-[11px] text-shade"
                        onClick={() => {
                            set_found(null);
                            set_query("");
                        }}
                    >
                        clear
                    </button>
                ) : null}
                <button
                    className={`rounded-md border px-2 py-0.5 font-mono text-[11px] ${
                        embedder?.reachable ? "border-palm text-palm" : "border-reef text-shade"
                    }`}
                    title={embedder?.detail ?? ""}
                    onClick={() => set_tuning((value) => !value)}
                >
                    {embedder?.reachable ? `vectors · ${embedder.dimensions}d` : "words only"}
                </button>
            </section>

            {tuning && embedder ? (
                <section className="flex shrink-0 flex-wrap items-center gap-1.5 rounded-md border border-reef px-2 py-1.5">
                    <span className="font-mono text-[10px] uppercase tracking-[0.14em] text-shade">
                        Embedder
                    </span>
                    <input
                        className="min-w-[210px] flex-1 rounded-md border border-reef bg-lagoon-deep font-mono text-[11px]"
                        placeholder="http://127.0.0.1:11434/v1/embeddings"
                        defaultValue={embedder.settings.endpoint ?? ""}
                        onBlur={(event) =>
                            run(async () => {
                                const report = await set_embedder({
                                    ...embedder.settings,
                                    endpoint: event.target.value.trim() || null,
                                });
                                set_embedder_report(report);
                            })
                        }
                    />
                    <input
                        className="w-36 rounded-md border border-reef bg-lagoon-deep font-mono text-[11px]"
                        defaultValue={embedder.settings.model}
                        onBlur={(event) =>
                            run(async () => {
                                const report = await set_embedder({
                                    ...embedder.settings,
                                    model: event.target.value.trim(),
                                });
                                set_embedder_report(report);
                            })
                        }
                    />
                    <label className="flex items-center gap-1 font-mono text-[11px] text-shell">
                        floor
                        <input
                            type="number"
                            step="0.05"
                            min="0"
                            max="1"
                            className="w-16 rounded-md border border-reef bg-lagoon-deep font-mono text-[11px]"
                            defaultValue={embedder.settings.min_similarity}
                            onBlur={(event) =>
                                run(async () => {
                                    const report = await set_embedder({
                                        ...embedder.settings,
                                        min_similarity: Number(event.target.value) || 0,
                                    });
                                    set_embedder_report(report);
                                })
                            }
                        />
                    </label>
                    <span className="font-mono text-[10px] text-shade">{embedder.detail}</span>
                </section>
            ) : null}

            {found ? (
                <section className="shrink-0">
                    <h3 className="mb-1 font-mono text-[9px] uppercase tracking-[0.14em] text-turquoise">
                        What an agent would be told · {found.length}
                    </h3>
                    {found.length === 0 ? (
                        <p className="font-mono text-[10px] text-shade">
                            Nothing matched. The brief would carry none of these.
                        </p>
                    ) : null}
                    <div className="flex flex-col gap-1">
                        {found.map((hit) => (
                            <article
                                key={hit.memory.id}
                                className="rounded-md border border-turquoise/50 bg-lagoon-deep px-2 py-1"
                            >
                                <div className="text-[12px] text-linen">{hit.memory.text}</div>
                                <div className="mt-0.5 flex gap-2 font-mono text-[10px] text-shade">
                                    <span className="text-turquoise">{hit.score.toFixed(2)}</span>
                                    <span>words {hit.lexical.toFixed(2)}</span>
                                    <span>vector {hit.semantic.toFixed(2)}</span>
                                </div>
                            </article>
                        ))}
                    </div>
                </section>
            ) : null}

            <section className="flex shrink-0 flex-wrap items-center gap-1.5">
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
                        <option key={scope.value} value={scope.value}>
                            {scope.label}
                        </option>
                    ))}
                </select>
                {draft.scope === "project" ? (
                    <input
                        className="w-36 rounded-md border border-reef bg-lagoon-deep font-mono text-[11px]"
                        placeholder="project id"
                        value={draft.project}
                        onChange={(event) => set_draft({ ...draft, project: event.target.value })}
                    />
                ) : null}
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

            <section className="shrink-0">
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
                            replaces={memory.supersedes ? by_slug.get(memory.supersedes) ?? null : null}
                            on_approve={() => run(() => answer_memory(memory.id, true))}
                            on_forget={() => run(() => forget_memory(memory.id))}
                        />
                    ))}
                </div>
            </section>

            {/* Every section sizes to its content and the panel scrolls. A
                section allowed to shrink below what is in it does not clip —
                it spills, and the next section is drawn over the top of it. */}
            <section className="shrink-0">
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
                            replaces={memory.supersedes ? by_slug.get(memory.supersedes) ?? null : null}
                            on_revoke={() => run(() => answer_memory(memory.id, false))}
                            on_forget={() => run(() => forget_memory(memory.id))}
                        />
                    ))}
                </div>
            </section>

            {retired.length > 0 ? (
                <section className="shrink-0">
                    <h3 className="mb-1 font-mono text-[9px] uppercase tracking-[0.14em] text-shade">
                        Taken back out · {retired.length}
                        <span className="ml-2 normal-case tracking-normal text-shade/70">
                            kept in the vault, told to nobody
                        </span>
                    </h3>
                    <div className="flex flex-col gap-1">
                        {retired.map((memory) => (
                            <Entry
                                key={memory.id}
                                memory={memory}
                                replaces={memory.supersedes ? by_slug.get(memory.supersedes) ?? null : null}
                                on_restore={() => run(() => answer_memory(memory.id, true))}
                                on_forget={() => run(() => forget_memory(memory.id))}
                            />
                        ))}
                    </div>
                </section>
            ) : null}
        </div>
    );
}

function Entry({
    memory,
    replaces,
    on_approve,
    on_revoke,
    on_restore,
    on_forget,
}: {
    memory: Memory;
    replaces?: Memory | null;
    on_approve?: () => void;
    on_revoke?: () => void;
    on_restore?: () => void;
    on_forget: () => void;
}) {
    const [asking, set_asking] = useState(false);

    return (
        <article
            className={`rounded-md border bg-lagoon-deep px-2 py-1 ${
                memory.approved
                    ? "border-reef"
                    : memory.retired
                      ? "border-reef/50 opacity-70"
                      : "border-coral/60"
            }`}
        >
            <div className="text-[12px] text-linen">{memory.text}</div>

            {replaces ? (
                <div className="mt-1 rounded border-l-2 border-sun/70 bg-lagoon px-2 py-1">
                    <div className="font-mono text-[9px] uppercase tracking-[0.12em] text-sun">
                        replaces {memory.approved ? "— already taken out of the brief" : "— approving this takes it out"}
                    </div>
                    <div className="text-[11px] text-shade line-clamp-2">{replaces.text}</div>
                </div>
            ) : null}

            <div className="mt-0.5 flex flex-wrap items-center gap-2 font-mono text-[10px] text-shade">
                <span title="the folder it lives in, inside the vault">{memory.scope}</span>
                <span>from {memory.proposed_by}</span>
                <span title={exactly(memory.written_at)}>
                    {when(memory.written_at, Math.floor(Date.now() / 1000))}
                </span>
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
                    {on_restore ? (
                        <button
                            className="rounded border border-palm px-1.5 text-palm"
                            title="tell the crew this again"
                            onClick={on_restore}
                        >
                            put back
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
                    {/* Deleting sits apart from the other two on purpose: revoke
                        takes a memory out of the crew's brief and keeps the file,
                        while this throws the file away. They were side by side and
                        indistinguishable, and the wrong one got clicked twice. */}
                    <span className="ml-1 border-l border-reef pl-1.5">
                        {asking ? (
                            <span className="flex items-center gap-1">
                                <span className="text-coral">delete the file?</span>
                                <button
                                    className="rounded border border-coral bg-coral/10 px-1.5 text-coral"
                                    onClick={() => {
                                        set_asking(false);
                                        on_forget();
                                    }}
                                >
                                    delete
                                </button>
                                <button
                                    className="rounded border border-reef px-1.5 hover:border-foam"
                                    onClick={() => set_asking(false)}
                                >
                                    keep
                                </button>
                            </span>
                        ) : (
                            <button
                                className="rounded border border-coral/40 px-1.5 text-coral/70 hover:border-coral hover:text-coral"
                                title="delete the note from the vault — there is no undo"
                                onClick={() => set_asking(true)}
                            >
                                forget
                            </button>
                        )}
                    </span>
                </span>
            </div>
        </article>
    );
}
