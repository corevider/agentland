import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";

import {
    attach_to_task,
    detach_from_task,
    set_marks,
    type Attachment,
    type Mark,
    type MarkKind,
    type Marks,
    type Task,
} from "@/lib/core";
import { derived_name, is_worth_keeping, MARK_TOOLS, paint } from "@/lib/marks";

/// The picture with the marks burned in, numbered, as a file for the card.
export async function flattened(
    image: HTMLImageElement,
    size: { width: number; height: number },
    marks: Mark[],
    name: string,
): Promise<File> {
    const flat = document.createElement("canvas");
    flat.width = size.width;
    flat.height = size.height;
    const ctx = flat.getContext("2d");
    if (!ctx) {
        throw new Error("this window cannot draw the marked copy");
    }
    ctx.drawImage(image, 0, 0, size.width, size.height);
    paint(ctx, { width: size.width, height: size.height, marks }, 1);

    const blob = await new Promise<Blob | null>((ok) => flat.toBlob(ok, "image/png"));
    if (!blob) {
        throw new Error("the marked copy came out empty");
    }
    return new File([blob], derived_name(name), { type: "image/png" });
}

/// A picture on a card, with a pen.
///
/// A box around the wrong thing says more than a paragraph about it. What is
/// drawn here is kept twice: as marks, in the picture's own pixels, so the
/// brief can say where each one is and what was said about it; and burned
/// into a copy of the picture, numbered, so the agent that opens the copy
/// sees what the person saw.
export function MarkupView({
    task_id,
    attachment,
    copy,
    load,
    on_close,
    on_saved,
    on_marked,
}: {
    /// The card the picture is on — or nothing, for a picture still staged
    /// in the editor, whose marks are handed back rather than saved.
    task_id: string | null;
    attachment: Pick<Attachment, "name" | "kind" | "marks">;
    /// The marked copy already on the card, if there is one.
    copy: Attachment | undefined;
    load: () => Promise<string>;
    on_close: () => void;
    on_saved?: (task: Task) => void;
    on_marked?: (marks: Marks, copy: File | null) => void;
}) {
    const [src, set_src] = useState<string | null>(null);
    const [natural, set_natural] = useState<{ width: number; height: number } | null>(null);
    const [shown, set_shown] = useState<{ width: number; height: number }>({ width: 0, height: 0 });
    const [marks, set_marks_here] = useState<Mark[]>(attachment.marks?.marks ?? []);
    const [tool, set_tool] = useState<MarkKind>("box");
    const [draft, set_draft_shown] = useState<Mark | null>(null);
    // The stroke in progress, read by the pointer handlers as it is, not as
    // it was when they were made: moves arrive faster than renders.
    const draft_now = useRef<Mark | null>(null);
    const set_draft = useCallback((next: Mark | null) => {
        draft_now.current = next;
        set_draft_shown(next);
    }, []);
    const [selected, set_selected] = useState<number | null>(null);
    const [busy, set_busy] = useState(false);
    const [error, set_error] = useState<string | null>(null);
    const image = useRef<HTMLImageElement>(null);
    const canvas = useRef<HTMLCanvasElement>(null);
    const words = useRef<HTMLInputElement>(null);

    const changed =
        JSON.stringify(marks) !== JSON.stringify(attachment.marks?.marks ?? []);

    // The picture is fetched once per attachment, not once per loader: the
    // loader is made afresh by every render of whoever opened this, and an
    // effect keyed on it revoked the URL before the picture had loaded.
    const loader = useRef(load);
    loader.current = load;
    useEffect(() => {
        let made: string | null = null;
        let gone = false;
        loader.current()
            .then((url) => {
                if (gone) {
                    URL.revokeObjectURL(url);
                } else {
                    made = url;
                    set_src(url);
                }
            })
            .catch((cause) => set_error(cause instanceof Error ? cause.message : String(cause)));
        return () => {
            gone = true;
            if (made) {
                URL.revokeObjectURL(made);
            }
        };
    }, [task_id, attachment.name]);
    const changed_or_new = changed || task_id === null;

    // The canvas sits over the picture at the picture's shown size, whatever
    // the window makes of it.
    useLayoutEffect(() => {
        const element = image.current;
        if (!element) {
            return;
        }
        const measure = () => set_shown({ width: element.clientWidth, height: element.clientHeight });
        measure();
        const watcher = new ResizeObserver(measure);
        watcher.observe(element);
        return () => watcher.disconnect();
    }, [src]);

    const scale = natural && shown.width > 0 ? shown.width / natural.width : 1;

    useEffect(() => {
        const element = canvas.current;
        if (!element || !natural || shown.width === 0) {
            return;
        }
        const ratio = window.devicePixelRatio || 1;
        element.width = Math.round(shown.width * ratio);
        element.height = Math.round(shown.height * ratio);
        const ctx = element.getContext("2d");
        if (!ctx) {
            return;
        }
        ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
        ctx.clearRect(0, 0, shown.width, shown.height);
        paint(ctx, { width: natural.width, height: natural.height, marks }, scale, { draft });
    }, [marks, draft, scale, natural, shown]);

    const point_of = useCallback(
        (event: React.PointerEvent<HTMLCanvasElement>): [number, number] => {
            const box = event.currentTarget.getBoundingClientRect();
            const x = (event.clientX - box.left) / scale;
            const y = (event.clientY - box.top) / scale;
            const clamp = (value: number, most: number) => Math.max(0, Math.min(most, value));
            return [clamp(x, natural?.width ?? x), clamp(y, natural?.height ?? y)];
        },
        [natural, scale],
    );

    const commit = useCallback((mark: Mark) => {
        if (!is_worth_keeping(mark)) {
            return;
        }
        set_marks_here((held) => {
            set_selected(held.length);
            return [...held, mark];
        });
        if (mark.kind === "label" || mark.kind === "pin" || mark.kind === "box") {
            window.setTimeout(() => words.current?.focus(), 0);
        }
    }, []);

    const save = useCallback(async () => {
        if (!natural || !image.current || busy) {
            return;
        }
        set_busy(true);
        set_error(null);

        try {
            const drawn: Marks = { width: natural.width, height: natural.height, marks };

            if (task_id === null) {
                const file = marks.length > 0 ? await flattened(image.current, natural, marks, attachment.name) : null;
                on_marked?.(drawn, file);
                return;
            }

            let task = await set_marks(task_id, attachment.name, drawn);

            if (marks.length === 0) {
                if (copy) {
                    task = await detach_from_task(task_id, copy.name);
                }
                on_saved?.(task);
                return;
            }

            const file = await flattened(image.current, natural, marks, attachment.name);
            task = await attach_to_task(task_id, file, attachment.name);
            on_saved?.(task);
        } catch (cause) {
            set_error(cause instanceof Error ? cause.message : String(cause));
        } finally {
            set_busy(false);
        }
    }, [attachment.name, busy, copy, marks, natural, on_marked, on_saved, task_id]);

    useEffect(() => {
        const key = (event: KeyboardEvent) => {
            const typing = event.target instanceof HTMLInputElement;
            if (event.key === "Escape") {
                if (draft) {
                    set_draft(null);
                } else if (typing) {
                    words.current?.blur();
                } else {
                    on_close();
                }
            } else if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
                void save();
            } else if ((event.key === "Delete" || event.key === "Backspace") && !typing && selected !== null) {
                set_marks_here((held) => held.filter((_, index) => index !== selected));
                set_selected(null);
            }
        };
        window.addEventListener("keydown", key);
        return () => window.removeEventListener("keydown", key);
    }, [draft, on_close, save, selected]);

    const current = selected !== null ? marks[selected] : undefined;
    const hint = MARK_TOOLS.find((entry) => entry.kind === tool)?.hint ?? "";

    return (
        <div className="fixed inset-0 z-[70] flex flex-col bg-lagoon-deep/95" onClick={(event) => event.stopPropagation()}>
            <header className="flex flex-wrap items-center gap-2 border-b border-reef px-3 py-1.5">
                <span className="mr-1 font-mono text-[11px] text-linen">{attachment.name}</span>
                {MARK_TOOLS.map((entry) => (
                    <button
                        key={entry.kind}
                        className={`rounded-lg border px-2 py-0.5 font-mono text-[11px] ${
                            tool === entry.kind ? "border-coral text-coral" : "border-reef text-shell hover:text-linen"
                        }`}
                        onClick={() => set_tool(entry.kind)}
                        title={entry.hint}
                    >
                        {entry.label}
                    </button>
                ))}
                <span className="font-mono text-[10px] text-shade">{hint}</span>
                <span className="flex-1" />
                <button
                    className="rounded-lg border border-reef px-2 py-0.5 font-mono text-[11px] text-shell disabled:opacity-40"
                    disabled={marks.length === 0}
                    onClick={() => {
                        set_marks_here((held) => held.slice(0, -1));
                        set_selected(null);
                    }}
                >
                    undo
                </button>
                <button
                    className="rounded-lg border border-reef px-2 py-0.5 font-mono text-[11px] text-shell disabled:opacity-40"
                    disabled={marks.length === 0}
                    onClick={() => {
                        set_marks_here([]);
                        set_selected(null);
                    }}
                >
                    clear
                </button>
                <button
                    className="rounded-lg border border-turquoise px-2 py-0.5 font-mono text-[11px] text-turquoise disabled:opacity-40"
                    disabled={busy || !changed_or_new || !natural}
                    onClick={() => void save()}
                    title="keep the marks on the card, and a numbered copy for the crew"
                >
                    {busy ? "saving…" : task_id === null ? "keep the marks" : "save the marks"}
                </button>
                <button
                    className="rounded px-1.5 font-mono text-[11px] text-shell hover:text-linen"
                    onClick={on_close}
                    title="close"
                >
                    ✕
                </button>
            </header>

            <div className="flex min-h-0 flex-1 items-center justify-center overflow-auto p-4">
                {src ? (
                    <div className="relative" style={{ lineHeight: 0 }}>
                        <img
                            ref={image}
                            src={src}
                            alt={attachment.name}
                            draggable={false}
                            className="max-h-[calc(100vh-130px)] max-w-[calc(100vw-40px)] select-none rounded-md border border-reef object-contain"
                            onLoad={(event) =>
                                set_natural({
                                    width: event.currentTarget.naturalWidth,
                                    height: event.currentTarget.naturalHeight,
                                })
                            }
                        />
                        <canvas
                            ref={canvas}
                            className="absolute left-0 top-0 touch-none"
                            style={{ width: shown.width, height: shown.height, cursor: "crosshair" }}
                            onPointerDown={(event) => {
                                if (event.button !== 0 || !natural) {
                                    return;
                                }
                                try {
                                    event.currentTarget.setPointerCapture(event.pointerId);
                                } catch {
                                    // A pointer the browser does not know — a synthetic one — cannot be captured, and need not be.
                                }
                                const point = point_of(event);
                                if (tool === "pin" || tool === "label") {
                                    commit({ kind: tool, points: [point], text: "" });
                                    return;
                                }
                                set_draft({ kind: tool, points: tool === "pen" ? [point] : [point, point], text: "" });
                            }}
                            onPointerMove={(event) => {
                                const held = draft_now.current;
                                if (!held) {
                                    return;
                                }
                                const point = point_of(event);
                                set_draft(
                                    held.kind === "pen"
                                        ? { ...held, points: [...held.points, point] }
                                        : { ...held, points: [held.points[0], point] },
                                );
                            }}
                            onPointerUp={() => {
                                const held = draft_now.current;
                                if (held) {
                                    commit(held);
                                    set_draft(null);
                                }
                            }}
                            onPointerCancel={() => set_draft(null)}
                        />
                    </div>
                ) : (
                    <div className="font-mono text-[11px] text-shade">{error ?? "fetching the picture…"}</div>
                )}
            </div>

            <footer className="flex flex-wrap items-center gap-2 border-t border-reef px-3 py-1.5">
                {current ? (
                    <>
                        <span className="font-mono text-[10px] text-coral">
                            mark {selected! + 1} · {current.kind}
                        </span>
                        <input
                            ref={words}
                            className="min-w-[240px] flex-1 rounded-md border border-reef bg-lagoon px-2 py-1 font-mono text-[11px] text-linen"
                            placeholder="what is this? the crew reads these words"
                            value={current.text}
                            onChange={(event) =>
                                set_marks_here((held) =>
                                    held.map((mark, index) => (index === selected ? { ...mark, text: event.target.value } : mark)),
                                )
                            }
                            onKeyDown={(event) => {
                                if (event.key === "Enter" && !event.ctrlKey && !event.metaKey) {
                                    event.currentTarget.blur();
                                }
                            }}
                        />
                        <button
                            className="rounded-lg border border-reef px-2 py-0.5 font-mono text-[11px] text-shell"
                            onClick={() => {
                                set_marks_here((held) => held.filter((_, index) => index !== selected));
                                set_selected(null);
                            }}
                        >
                            remove this mark
                        </button>
                    </>
                ) : (
                    <span className="font-mono text-[10px] text-shade">
                        {marks.length === 0
                            ? "nothing drawn yet — pick a tool and draw on the picture"
                            : `${marks.length} mark${marks.length === 1 ? "" : "s"} · click a number's tool again to add more · ctrl+enter saves`}
                    </span>
                )}
                {marks.length > 0 && !current ? (
                    <select
                        className="rounded-lg border border-reef bg-lagoon px-2 py-0.5 font-mono text-[11px] text-linen"
                        value=""
                        onChange={(event) => set_selected(event.target.value === "" ? null : Number(event.target.value))}
                    >
                        <option value="">edit a mark…</option>
                        {marks.map((mark, index) => (
                            <option key={index} value={index}>
                                {index + 1} · {mark.kind}
                                {mark.text.trim() ? ` · ${mark.text.trim().slice(0, 30)}` : ""}
                            </option>
                        ))}
                    </select>
                ) : null}
                {error ? <span className="font-mono text-[11px] text-coral">{error}</span> : null}
            </footer>
        </div>
    );
}
