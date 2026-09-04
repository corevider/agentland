import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { files_from_drop, files_from_paste } from "@/lib/attachments";
import {
    attach_to_task,
    attachment_object_url,
    create_task,
    detach_from_task,
    edit_task,
    set_marks,
    type Attachment,
    type Marks,
    type Repository,
    type Task,
} from "@/lib/core";

import { derived_name, marked_copy_of, originals } from "@/lib/marks";

import { AttachmentTile, Lightbox } from "./Attachments";
import { MarkupView } from "./Markup";

/// A file staged in the window, with a key of its own so two files with one
/// name can both be taken off again.
interface Staged {
    key: number;
    file: File;
    /// What was drawn on it before the card existed, and the marked copy
    /// made of it, both put on the card once it does.
    marks?: Marks;
    copy?: File | null;
}

let next_key = 1;

function stage(files: File[]): Staged[] {
    return files.map((file) => ({ key: next_key++, file }));
}

/// Writing a card, or rewriting one.
///
/// A card used to be a title and a line of brief typed into a bar above the
/// board, and nothing more could be said about it afterwards. This is the
/// whole card: what it asks, which project it is for, and the files that show
/// it — a screenshot pasted straight from the clipboard, a design dropped from
/// the desktop. Files on a card reach whoever takes it: their paths are part
/// of the brief the agent is handed.
export function CardEditor({
    task,
    repos,
    default_repository,
    seed,
    on_close,
    on_saved,
}: {
    /// The card being rewritten, or nothing for a new one.
    task: Task | null;
    repos: Repository[];
    default_repository: string;
    /// Files that arrived before the editor did — a paste onto the board.
    seed: File[];
    on_close: () => void;
    on_saved: (task: Task) => void;
}) {
    const [title, set_title] = useState(task?.title ?? "");
    const [body, set_body] = useState(task?.body ?? "");
    const [repository_id, set_repository] = useState(task?.repository_id ?? default_repository);
    const [kept, set_kept] = useState<Attachment[]>(originals(task?.attachments));
    const [all_on_card, set_all_on_card] = useState<Attachment[]>(task?.attachments ?? []);
    const [marking, set_marking] = useState<Attachment | null>(null);
    const [marking_staged, set_marking_staged] = useState<Staged | null>(null);
    const [added, set_added] = useState<Staged[]>([]);
    const [over, set_over] = useState(false);
    const [busy, set_busy] = useState(false);
    const [error, set_error] = useState<string | null>(null);
    const [shown, set_shown] = useState<{ id: string; load: () => Promise<string>; name: string } | null>(null);
    const title_box = useRef<HTMLInputElement>(null);
    const chooser = useRef<HTMLInputElement>(null);

    useEffect(() => {
        title_box.current?.focus();
    }, []);

    const removed = useMemo(
        () => originals(task?.attachments).filter((held) => !kept.some((still) => still.name === held.name)),
        [kept, task],
    );

    const take_files = useCallback((files: File[]) => {
        if (files.length > 0) {
            set_added((held) => [...held, ...stage(files)]);
        }
    }, []);

    // Files that arrive from outside the panel — a paste with the focus
    // elsewhere — are appended to the seed, and staged as they come.
    const seeded = useRef(0);
    useEffect(() => {
        if (seed.length > seeded.current) {
            take_files(seed.slice(seeded.current));
            seeded.current = seed.length;
        }
    }, [seed, take_files]);

    // A staged file is shown straight from the file: an object URL of its
    // own for every picture drawn, which the picture lets go of itself. It
    // was fetched and copied once, and the window's content policy refused
    // the fetch — a blob is not one of the places it may connect to.
    const preview_of = useCallback((entry: Staged) => {
        return () => Promise.resolve(URL.createObjectURL(entry.file));
    }, []);

    const loader_for = useCallback(
        (held: Attachment) => () => {
            if (!task) {
                return Promise.reject(new Error("no card yet"));
            }
            return attachment_object_url(task.id, held.name);
        },
        [task],
    );

    const can_save = !busy && title.trim().length > 0 && repository_id.length > 0;

    const save = useCallback(async () => {
        if (!can_save) {
            return;
        }

        set_busy(true);
        set_error(null);

        try {
            let card: Task;
            if (task) {
                const change: { title?: string; body?: string } = {};
                if (title.trim() !== task.title) {
                    change.title = title.trim();
                }
                if (body !== task.body) {
                    change.body = body;
                }
                card = Object.keys(change).length > 0 ? await edit_task(task.id, change) : task;

                for (const gone of removed) {
                    card = await detach_from_task(task.id, gone.name);
                }
            } else {
                card = await create_task(title.trim(), body, repository_id);
            }

            for (const entry of added) {
                card = await attach_to_task(card.id, entry.file);
                const landed = card.attachments?.at(-1)?.name ?? entry.file.name;
                if (entry.marks && entry.marks.marks.length > 0) {
                    card = await set_marks(card.id, landed, entry.marks);
                    if (entry.copy) {
                        const copy = new File([entry.copy], derived_name(landed), { type: "image/png" });
                        card = await attach_to_task(card.id, copy, landed);
                    }
                }
            }

            on_saved(card);
        } catch (cause) {
            set_error(cause instanceof Error ? cause.message : String(cause));
        } finally {
            set_busy(false);
        }
    }, [added, body, can_save, on_saved, removed, repository_id, task, title]);

    return (
        <aside
            className={`relative flex w-full min-w-0 flex-col border-l transition-colors @[820px]:w-[46%] @[820px]:min-w-[380px] ${
                over ? "border-turquoise bg-lagoon-deep" : "border-reef"
            }`}
            onPaste={(event) => {
                const files = files_from_paste(event.clipboardData);
                if (files.length > 0) {
                    event.preventDefault();
                    take_files(files);
                }
            }}
            onDragOver={(event) => {
                event.preventDefault();
                event.dataTransfer.dropEffect = "copy";
                set_over(true);
            }}
            onDragLeave={(event) => {
                if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
                    set_over(false);
                }
            }}
            onDrop={(event) => {
                event.preventDefault();
                set_over(false);
                take_files(files_from_drop(event.dataTransfer));
            }}
            onKeyDown={(event) => {
                if (event.key === "Escape") {
                    on_close();
                } else if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
                    void save();
                }
            }}
        >
            <header className="flex items-start justify-between gap-2 border-b border-reef px-2 py-1.5">
                <div className="min-w-0">
                    <div className="text-[12px] text-linen">{task ? `rewriting ${task.id}` : "a new card"}</div>
                    <div className="font-mono text-[10px] text-shade">
                        paste a screenshot, or drop files anywhere on this panel · click a picture to draw on it
                    </div>
                </div>
                <button
                    className="rounded px-1.5 font-mono text-[11px] text-shell hover:text-linen"
                    onClick={on_close}
                    title="close without saving"
                >
                    ✕
                </button>
            </header>

            <div className="flex min-h-0 flex-1 flex-col gap-2.5 overflow-y-auto p-2.5">
                <input
                    ref={title_box}
                    className="rounded-md border border-reef bg-lagoon px-2 py-1 font-mono text-[11px] text-linen"
                    placeholder="what needs doing"
                    value={title}
                    onChange={(event) => set_title(event.target.value)}
                />

                <textarea
                    className="min-h-[140px] resize-y rounded-md border border-reef bg-lagoon px-2 py-1 font-mono text-[11px] leading-relaxed text-linen"
                    placeholder="the brief for whoever takes it — what, where, how you will know it is done"
                    value={body}
                    onChange={(event) => set_body(event.target.value)}
                />

                {task ? (
                    <div className="font-mono text-[10px] text-shade">
                        project · {task.repository_id}
                    </div>
                ) : (
                    <label className="flex items-center gap-2 font-mono text-[10px] text-shade">
                        project
                        <select
                            className="rounded-lg border border-reef bg-lagoon px-2 py-1 font-mono text-[11px] text-linen"
                            value={repository_id}
                            onChange={(event) => set_repository(event.target.value)}
                        >
                            {repos.map((repo) => (
                                <option key={repo.id} value={repo.id}>
                                    {repo.name}
                                </option>
                            ))}
                        </select>
                    </label>
                )}

                <section>
                    <div className="mb-1 flex items-baseline justify-between">
                        <h3 className="font-mono text-[9px] uppercase tracking-[0.14em] text-shade">
                            Attached · {kept.length + added.length}
                        </h3>
                        <button
                            className="font-mono text-[10px] text-turquoise hover:underline"
                            onClick={() => chooser.current?.click()}
                        >
                            choose files…
                        </button>
                        <input
                            ref={chooser}
                            type="file"
                            multiple
                            className="hidden"
                            onChange={(event) => {
                                take_files(Array.from(event.target.files ?? []));
                                event.target.value = "";
                            }}
                        />
                    </div>

                    {kept.length + added.length === 0 ? (
                        <div
                            className={`rounded-md border border-dashed px-2 py-4 text-center font-mono text-[10px] ${
                                over ? "border-turquoise text-turquoise" : "border-reef text-shade"
                            }`}
                        >
                            nothing yet — ctrl+v a screenshot, or drop a file here
                        </div>
                    ) : (
                        <div className="flex flex-wrap gap-1.5">
                            {kept.map((held) => (
                                <AttachmentTile
                                    key={`kept-${held.name}`}
                                    id={`${task?.id}/${held.name}`}
                                    name={held.name}
                                    kind={held.kind}
                                    bytes={held.bytes}
                                    load={loader_for(held)}
                                    marked={held.marks?.marks.length}
                                    on_open={() =>
                                        held.kind.startsWith("image/")
                                            ? set_marking(held)
                                            : set_shown({ id: `${task?.id}/${held.name}`, load: loader_for(held), name: held.name })
                                    }
                                    on_remove={() =>
                                        set_kept((still) => still.filter((entry) => entry.name !== held.name))
                                    }
                                />
                            ))}
                            {added.map((entry) => (
                                <AttachmentTile
                                    key={`added-${entry.key}`}
                                    id={`staged/${entry.key}`}
                                    name={entry.file.name}
                                    kind={entry.file.type || "application/octet-stream"}
                                    bytes={entry.file.size}
                                    load={preview_of(entry)}
                                    marked={entry.marks?.marks.length}
                                    on_open={() =>
                                        entry.file.type.startsWith("image/")
                                            ? set_marking_staged(entry)
                                            : set_shown({ id: `staged/${entry.key}`, load: preview_of(entry), name: entry.file.name })
                                    }
                                    on_remove={() =>
                                        set_added((held) => held.filter((other) => other.key !== entry.key))
                                    }
                                    pending
                                />
                            ))}
                        </div>
                    )}
                </section>

                {error ? (
                    <div className="rounded-lg border border-coral bg-lagoon px-2 py-1 font-mono text-[11px] text-coral">
                        {error}
                    </div>
                ) : null}
            </div>

            <footer className="flex items-center justify-between gap-2 border-t border-reef px-2.5 py-1.5">
                <span className="font-mono text-[10px] text-shade">ctrl+enter saves · esc closes</span>
                <div className="flex gap-2">
                    <button
                        className="rounded-lg border border-reef px-2 py-1 font-mono text-[11px] text-shell"
                        onClick={on_close}
                        disabled={busy}
                    >
                        cancel
                    </button>
                    <button
                        className="rounded-lg border border-turquoise px-2 py-1 font-mono text-[11px] text-turquoise disabled:opacity-40"
                        disabled={!can_save}
                        onClick={() => void save()}
                    >
                        {busy ? "saving…" : task ? "save the card" : "put it on the board"}
                    </button>
                </div>
            </footer>

            {shown ? (
                <Lightbox id={shown.id} load={shown.load} alt={shown.name} on_close={() => set_shown(null)} />
            ) : null}

            {marking_staged ? (
                <MarkupView
                    task_id={null}
                    attachment={{
                        name: marking_staged.file.name,
                        kind: marking_staged.file.type || "image/png",
                        marks: marking_staged.marks,
                    }}
                    copy={undefined}
                    load={preview_of(marking_staged)}
                    on_close={() => set_marking_staged(null)}
                    on_marked={(marks, copy) => {
                        set_marking_staged(null);
                        set_added((held) =>
                            held.map((entry) =>
                                entry.key === marking_staged.key ? { ...entry, marks, copy } : entry,
                            ),
                        );
                    }}
                />
            ) : null}

            {marking && task ? (
                <MarkupView
                    task_id={task.id}
                    attachment={marking}
                    copy={marked_copy_of(all_on_card, marking.name)}
                    load={loader_for(marking)}
                    on_close={() => set_marking(null)}
                    on_saved={(saved) => {
                        set_marking(null);
                        set_all_on_card(saved.attachments ?? []);
                        set_kept((still) =>
                            originals(saved.attachments).filter((held) => still.some((entry) => entry.name === held.name)),
                        );
                    }}
                />
            ) : null}
        </aside>
    );
}
