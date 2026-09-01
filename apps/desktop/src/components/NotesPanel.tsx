import { useCallback, useState } from "react";

import {
    forget_note,
    list_notes,
    read_note,
    read_vault,
    write_note,
    type Note,
    type VaultReport,
} from "@/lib/core";
import { use_poll } from "@/lib/poll";
import { exactly, when } from "@/lib/when";

/// What the crew has written down.
///
/// The vault is a folder of markdown files with `[[links]]` between them, so it
/// opens in any note tool and a note edited by hand is read back by the crew. A
/// note is a record, never an instruction: what an agent reads here it quotes,
/// it does not obey.
///
/// A few of these notes are memories — one-line facts an approved agent is told
/// without having to look them up. They live in the same vault, in a `memory/`
/// folder under their scope, and are marked here so the difference between "the
/// crew can find this" and "the crew is told this" is visible in one list.
export function NotesPanel({ active }: { active: boolean }) {
    const [notes, set_notes] = useState<Note[]>([]);
    const [query, set_query] = useState("");
    const [open, set_open] = useState<Note | null>(null);
    const [draft, set_draft] = useState({ title: "", body: "", tags: "" });
    const [notice, set_notice] = useState<string | null>(null);
    const [deleting, set_deleting] = useState<string | null>(null);
    const [vault, set_vault] = useState<VaultReport | null>(null);

    const refresh = useCallback(() => {
        list_notes(query)
            .then((held) => {
                set_notes(held);
                // A vault panel showing nothing until you click is a shelf with
                // the lights off: open the first note so the shape of a note is
                // visible without asking.
                set_open((shown) => shown ?? held[0] ?? null);
            })
            .catch((cause) => set_notice(cause instanceof Error ? cause.message : String(cause)));
    }, [query]);

    use_poll(refresh, 5000, active);
    use_poll(() => {
        read_vault().then(set_vault).catch(() => undefined);
    }, 20000, active);

    const show = (slug: string) => {
        read_note(slug)
            .then(set_open)
            .catch((cause) => set_notice(String(cause)));
    };

    return (
        <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-2 overflow-y-auto p-2.5">
            <section className="flex flex-wrap items-center gap-2">
                <input
                    className="min-w-[12rem] flex-1 rounded-md border border-reef bg-lagoon-deep px-2 py-1 text-[12px] text-linen"
                    placeholder="what did we write about…"
                    value={query}
                    onChange={(event) => set_query(event.target.value)}
                    onKeyDown={(event) => event.key === "Enter" && refresh()}
                />
                <button
                    className="rounded-md border border-reef px-2 py-1 font-mono text-[11px] text-shell hover:border-foam"
                    onClick={refresh}
                >
                    search
                </button>
                <span className="font-mono text-[10px] text-shade">
                    {notes.length} note{notes.length === 1 ? "" : "s"}
                </span>
            </section>

            {vault ? (
                <p
                    className="cursor-text select-text font-mono text-[10px] text-shade"
                    title="open this folder in Obsidian, or any other note tool"
                >
                    {vault.path}
                </p>
            ) : null}

            {notice ? (
                <div className="rounded-md border border-coral px-2 py-1 font-mono text-[11px] text-coral">
                    {notice}
                </div>
            ) : null}

            <section className="flex flex-col gap-1">
                {notes.length === 0 ? (
                    <p className="font-mono text-[10px] text-shade">
                        Nothing written down yet. An agent writes one with note_write, and you can drop
                        a markdown file into the vault by hand.
                    </p>
                ) : null}

                {notes.map((note) => (
                    <article
                        key={note.slug}
                        className={`cursor-pointer rounded-md border bg-lagoon-deep px-2 py-1 ${
                            open?.slug === note.slug ? "border-turquoise" : "border-reef"
                        }`}
                        onClick={() => show(note.slug)}
                    >
                        <div className="flex flex-wrap items-baseline gap-2">
                            <span className="text-[12px] text-linen">{note.title}</span>
                            {note.approved === true ? (
                                <span
                                    className="rounded border border-turquoise px-1 font-mono text-[9px] text-turquoise"
                                    title="a memory: agents in this scope are told it, without looking"
                                >
                                    briefed
                                </span>
                            ) : null}
                            {note.approved === false ? (
                                <span
                                    className="rounded border border-coral px-1 font-mono text-[9px] text-coral"
                                    title="a memory waiting on you — approve it in Memory before anyone is told"
                                >
                                    waiting on you
                                </span>
                            ) : null}
                            {note.tags.map((tag) => (
                                <span key={tag} className="rounded bg-lagoon px-1 font-mono text-[9px] text-shell">
                                    {tag}
                                </span>
                            ))}
                            <span className="ml-auto font-mono text-[10px] text-shade">
                                <span title={exactly(note.written_at)}>
                                    {when(note.written_at, Math.floor(Date.now() / 1000))}
                                </span>
                                {" · "}
                                {note.written_by || "someone"}
                                {note.backlinks.length > 0 ? ` · ${note.backlinks.length} back` : ""}
                            </span>
                        </div>
                    </article>
                ))}
            </section>

            {open ? (
                <section className="rounded-md border border-turquoise bg-lagoon-deep p-2">
                    <div className="flex items-baseline gap-2">
                        <h3 className="text-[12px] text-linen">{open.title}</h3>
                        <span className="font-mono text-[10px] text-shade">{open.slug}.md</span>
                        <span className="font-mono text-[10px] text-shade" title={exactly(open.written_at)}>
                            {when(open.written_at, Math.floor(Date.now() / 1000))}
                        </span>
                        {/* The vault is the only copy: a note deleted here is
                            gone from disk. One click asks, the second deletes. */}
                        {deleting === open.slug ? (
                            <span className="ml-auto flex items-center gap-1 font-mono text-[10px]">
                                <span className="text-coral">delete {open.slug}.md?</span>
                                <button
                                    className="rounded border border-coral bg-coral/10 px-1.5 text-coral"
                                    onClick={() => {
                                        set_deleting(null);
                                        forget_note(open.slug)
                                            .then(() => {
                                                set_open(null);
                                                refresh();
                                            })
                                            .catch((cause) => set_notice(String(cause)));
                                    }}
                                >
                                    delete
                                </button>
                                <button
                                    className="rounded border border-reef px-1.5 text-shell hover:border-foam"
                                    onClick={() => set_deleting(null)}
                                >
                                    keep
                                </button>
                            </span>
                        ) : (
                            <button
                                className="ml-auto rounded px-1 font-mono text-[11px] text-shade hover:text-coral"
                                title="delete this note from the vault — there is no undo"
                                onClick={() => set_deleting(open.slug)}
                            >
                                ×
                            </button>
                        )}
                    </div>

                    <pre className="mt-1 whitespace-pre-wrap break-words font-mono text-[11px] leading-relaxed text-driftwood">
                        {open.body}
                    </pre>

                    {open.links.length > 0 || open.backlinks.length > 0 ? (
                        <div className="mt-2 flex flex-col gap-1 border-t border-reef/70 pt-1 font-mono text-[10px]">
                            {open.links.length > 0 ? (
                                <div className="flex flex-wrap items-center gap-1">
                                    <span className="text-shade">points at</span>
                                    {open.links.map((slug) => (
                                        <button
                                            key={slug}
                                            className="rounded border border-reef px-1 text-turquoise hover:border-turquoise"
                                            onClick={() => show(slug)}
                                        >
                                            {slug}
                                        </button>
                                    ))}
                                </div>
                            ) : null}

                            {open.backlinks.length > 0 ? (
                                <div className="flex flex-wrap items-center gap-1">
                                    <span className="text-shade">pointed at by</span>
                                    {open.backlinks.map((slug) => (
                                        <button
                                            key={slug}
                                            className="rounded border border-reef px-1 text-shell hover:border-turquoise"
                                            onClick={() => show(slug)}
                                        >
                                            {slug}
                                        </button>
                                    ))}
                                </div>
                            ) : null}
                        </div>
                    ) : null}
                </section>
            ) : null}

            <section className="rounded-md border border-reef bg-lagoon-deep p-2">
                <h3 className="mb-1 font-mono text-[9px] uppercase tracking-[0.14em] text-shade">
                    Write one yourself
                </h3>
                <div className="flex flex-col gap-1">
                    <input
                        className="rounded-md border border-reef bg-lagoon px-2 py-1 text-[12px] text-linen"
                        placeholder="title"
                        value={draft.title}
                        onChange={(event) => set_draft({ ...draft, title: event.target.value })}
                    />
                    <textarea
                        className="min-h-[4rem] rounded-md border border-reef bg-lagoon px-2 py-1 font-mono text-[11px] text-linen"
                        placeholder="markdown — point at another note with [[its title]]"
                        value={draft.body}
                        onChange={(event) => set_draft({ ...draft, body: event.target.value })}
                    />
                    <div className="flex gap-2">
                        <input
                            className="flex-1 rounded-md border border-reef bg-lagoon px-2 py-1 font-mono text-[11px] text-linen"
                            placeholder="tags, comma separated"
                            value={draft.tags}
                            onChange={(event) => set_draft({ ...draft, tags: event.target.value })}
                        />
                        <button
                            className="rounded-md border border-turquoise px-2 py-1 font-mono text-[11px] text-turquoise disabled:opacity-40"
                            disabled={!draft.title.trim() || !draft.body.trim()}
                            onClick={() => {
                                write_note({
                                    title: draft.title,
                                    body: draft.body,
                                    tags: draft.tags
                                        .split(",")
                                        .map((tag) => tag.trim())
                                        .filter(Boolean),
                                    written_by: "you",
                                })
                                    .then((written) => {
                                        set_draft({ title: "", body: "", tags: "" });
                                        set_open(written);
                                        refresh();
                                    })
                                    .catch((cause) => set_notice(String(cause)));
                            }}
                        >
                            write
                        </button>
                    </div>
                </div>
            </section>
        </div>
    );
}
