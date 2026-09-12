import { useMemo, useState } from "react";

import { anchor_of, pinnable, read_patch, type Note, type PatchLine } from "@/lib/annotations";

const TINT: Record<PatchLine["kind"], string> = {
    file: "text-shell",
    meta: "text-shade",
    hunk: "text-turquoise",
    added: "text-palm",
    removed: "text-coral",
    context: "text-driftwood",
};

const same_place = (note: Note, line: PatchLine) => {
    const anchor = anchor_of(line);
    return note.file === anchor.file && note.line === anchor.line && note.side === anchor.side;
};

/// A diff a person can pin notes to, line by line.
///
/// Hover a line and a + appears beside it; the note is written under the line
/// and stays pinned there until it is sent or taken off. Nothing leaves the
/// panel until every note goes back at once. Without `on_add` it is only a
/// diff to read, as it is when several are laid side by side.
export function AnnotatedPatch({
    patch,
    notes = [],
    on_add,
    on_remove,
}: {
    patch: string;
    notes?: Note[];
    on_add?: (note: Note) => void;
    on_remove?: (index: number) => void;
}) {
    const lines = useMemo(() => read_patch(patch), [patch]);
    const [writing, set_writing] = useState<number | null>(null);
    const [draft, set_draft] = useState("");

    const save = (line: PatchLine) => {
        const text = draft.trim();
        if (text) {
            on_add?.({ ...anchor_of(line), text });
        }
        set_writing(null);
        set_draft("");
    };

    return (
        <div className="min-h-0 flex-1 overflow-auto p-2 font-mono text-[11px] leading-relaxed">
            {lines.map((line, index) => (
                <div key={index}>
                    <div className={`group flex ${TINT[line.kind]}`}>
                        {on_add ? (
                            <button
                                className="w-4 shrink-0 text-center text-shade opacity-0 hover:text-turquoise group-hover:opacity-100 disabled:invisible"
                                disabled={!pinnable(line)}
                                title="pin a note to this line"
                                onClick={() => {
                                    set_writing(index);
                                    set_draft("");
                                }}
                            >
                                +
                            </button>
                        ) : null}
                        <span className="min-w-0 whitespace-pre">{line.text || " "}</span>
                    </div>

                    {notes.map((note, at) =>
                        pinnable(line) && same_place(note, line) ? (
                            <div
                                key={at}
                                className="my-0.5 ml-4 flex items-start justify-between gap-2 rounded-md border border-sun/60 bg-lagoon-deep px-2 py-1"
                            >
                                <span className="whitespace-pre-wrap font-sans text-[11px] text-linen">{note.text}</span>
                                <button
                                    className="shrink-0 text-[10px] text-shade hover:text-coral"
                                    title="take this note off"
                                    onClick={() => on_remove?.(at)}
                                >
                                    ✕
                                </button>
                            </div>
                        ) : null,
                    )}

                    {writing === index ? (
                        <div className="my-1 ml-4 flex flex-col gap-1">
                            <textarea
                                autoFocus
                                value={draft}
                                onChange={(event) => set_draft(event.target.value)}
                                onKeyDown={(event) => {
                                    if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
                                        event.preventDefault();
                                        save(line);
                                    } else if (event.key === "Escape") {
                                        event.preventDefault();
                                        set_writing(null);
                                    }
                                }}
                                placeholder="what has to change here"
                                className="min-h-[3.5rem] rounded-md border border-turquoise bg-lagoon-deep p-1 font-sans text-[11px] text-linen"
                            />
                            <div className="flex gap-1 text-[10px]">
                                <button
                                    className="rounded-lg border border-turquoise px-1.5 text-turquoise"
                                    onClick={() => save(line)}
                                >
                                    pin note · ctrl+enter
                                </button>
                                <button
                                    className="rounded-lg border border-reef px-1.5 text-shell"
                                    onClick={() => set_writing(null)}
                                >
                                    cancel · esc
                                </button>
                            </div>
                        </div>
                    ) : null}
                </div>
            ))}
        </div>
    );
}
