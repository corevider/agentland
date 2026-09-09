import { useCallback, useEffect, useState } from "react";

import { Waiting } from "@/components/Spinner";
import {
    forget_standards,
    read_saved_standards,
    read_standards,
    restore_standards,
    set_standards,
    type HouseRules,
    type SavedRules,
} from "@/lib/core";
import { exactly, when } from "@/lib/when";

/// How the house works.
///
/// Not a brief and not a memory: a brief is one piece of work and a memory is
/// something the crew learned. This is what holds for everybody, every turn —
/// naming, commit messages, what never goes in a log.
///
/// Every save is kept. Rules are edited in a box at the moment somebody is
/// annoyed about something, which is exactly when the paragraph that was
/// holding the crew together goes out with the one that was not.
export function StandardsSection() {
    const [draft, set_draft] = useState<string | null>(null);
    const [rules, set_rules] = useState<HouseRules | null>(null);
    const [notice, set_notice] = useState<string | null>(null);
    const [saying, set_saying] = useState<string | null>(null);
    const [saving, set_saving] = useState(false);
    const [showing, set_showing] = useState<{ id: string; text: string } | null>(null);
    const [now, set_now] = useState(() => Math.floor(Date.now() / 1000));

    const refresh = useCallback(async () => {
        const held = await read_standards();
        set_rules(held);
        set_draft((current) => current ?? held.text);
        set_now(Math.floor(Date.now() / 1000));
    }, []);

    useEffect(() => {
        refresh().catch((cause) => set_notice(String(cause)));
    }, [refresh]);

    const run = useCallback(
        async (action: () => Promise<HouseRules>, say?: string) => {
            try {
                set_notice(null);
                const held = await action();
                set_rules(held);
                if (say) {
                    set_saying(say);
                    window.setTimeout(() => set_saying(null), 2000);
                }
                return held;
            } catch (cause) {
                set_notice(cause instanceof Error ? cause.message : String(cause));
                return null;
            }
        },
        [],
    );

    const save = useCallback(async () => {
        if (draft === null) {
            return;
        }

        set_saving(true);
        await run(() => set_standards(draft), "saved");
        set_saving(false);
    }, [draft, run]);

    const copy = useCallback(async () => {
        if (draft === null) {
            return;
        }

        try {
            await navigator.clipboard.writeText(draft);
            set_saying("copied");
            window.setTimeout(() => set_saying(null), 2000);
        } catch (cause) {
            set_notice(cause instanceof Error ? cause.message : String(cause));
        }
    }, [draft]);

    const show = useCallback(
        async (held: SavedRules) => {
            if (showing?.id === held.id) {
                set_showing(null);
                return;
            }

            try {
                set_notice(null);
                const page = await read_saved_standards(held.id);
                set_showing({ id: held.id, text: page.text });
            } catch (cause) {
                set_notice(cause instanceof Error ? cause.message : String(cause));
            }
        },
        [showing],
    );

    const put_back = useCallback(
        async (held: SavedRules) => {
            const next = await run(() => restore_standards(held.id), "put back");
            if (next) {
                set_draft(next.text);
                set_showing(null);
            }
        },
        [run],
    );

    const forget = useCallback(
        async (held: SavedRules) => {
            await run(() => forget_standards(held.id), "forgotten");
            set_showing((open) => (open?.id === held.id ? null : open));
        },
        [run],
    );

    if (draft === null || rules === null) {
        return <Waiting says="reading the house rules…" className="font-mono text-[11px] text-shade" />;
    }

    const unsaved = draft.trim() !== rules.text.trim();

    return (
        <section className="flex min-h-0 flex-1 flex-col gap-3">
            <p className="font-mono text-[11px] text-shade">
                Handed to every agent, in every project, on every turn. Claude Code is given the
                file itself, so it costs nothing to repeat; an engine that takes no standing
                instruction is told at the top of its brief instead.
            </p>

            <p className="font-mono text-[10px] text-shade">
                A new machine starts with a page somebody would have written anyway — it is meant
                to be edited, and emptying it means no rules rather than the page coming back.
            </p>

            <textarea
                className="h-[22rem] min-h-[10rem] resize-y rounded-lg border border-reef bg-lagoon p-2 font-mono text-[11px] leading-relaxed"
                spellCheck={false}
                placeholder={"# House rules\n\n- Four spaces, and names that say what they are.\n- Tests with behaviour changes.\n- Never a secret in a log."}
                value={draft}
                onChange={(event) => set_draft(event.target.value)}
            />

            <div className="flex flex-wrap items-center gap-2">
                <button
                    className="rounded-lg border border-turquoise px-2 py-1 font-mono text-[11px] text-turquoise disabled:opacity-40"
                    disabled={saving}
                    onClick={() => void save()}
                >
                    {saving ? "saving…" : "save"}
                </button>
                <button
                    className="rounded-lg border border-reef px-2 py-1 font-mono text-[11px] text-shell hover:border-turquoise hover:text-turquoise"
                    title="the page as it stands in the box, on the clipboard"
                    onClick={() => void copy()}
                >
                    copy
                </button>
                {unsaved ? (
                    <span className="font-mono text-[10px] text-sun">unsaved</span>
                ) : null}
                {saying ? <span className="font-mono text-[10px] text-palm">{saying}</span> : null}
                <span className="font-mono text-[10px] text-shade">
                    {rules.held
                        ? "in force — agents started from now on are handed it"
                        : "nothing set; agents are told only what their work needs"}
                </span>
            </div>

            {notice ? <p className="font-mono text-[11px] text-coral">{notice}</p> : null}

            <div className="flex min-h-0 flex-col gap-1.5">
                <span className="font-mono text-[9px] uppercase tracking-[0.14em] text-shade">
                    Every save, newest first
                </span>

                {rules.saved.length === 0 ? (
                    <p className="font-mono text-[10px] text-shade">
                        Nothing saved yet. From the next save on, each one is kept here — twenty of
                        them, and the oldest goes when the twenty-first arrives.
                    </p>
                ) : null}

                {rules.saved.map((held) => (
                    <article
                        key={held.id}
                        className={`flex flex-col gap-1 rounded-md border px-2 py-1 ${
                            held.current ? "border-turquoise" : "border-reef"
                        }`}
                    >
                        <div className="flex flex-wrap items-baseline gap-2">
                            <span
                                className="cursor-pointer font-mono text-[11px] text-shell hover:text-turquoise"
                                role="button"
                                tabIndex={0}
                                title={exactly(held.at)}
                                onClick={() => void show(held)}
                                onKeyDown={(event) => {
                                    if (event.key === "Enter" || event.key === " ") {
                                        event.preventDefault();
                                        void show(held);
                                    }
                                }}
                            >
                                {showing?.id === held.id ? "▾" : "▸"} {when(held.at, now)}
                            </span>
                            <span className="min-w-0 flex-1 truncate font-mono text-[10px] text-driftwood">
                                {held.opens}
                            </span>
                            <span className="font-mono text-[10px] text-shade">
                                {held.characters.toLocaleString()} characters
                            </span>
                            {held.current ? (
                                <span className="font-mono text-[10px] text-turquoise">in force</span>
                            ) : null}
                            <span className="flex items-center gap-1">
                                <button
                                    className="rounded border border-reef px-1.5 font-mono text-[10px] text-shell hover:border-turquoise hover:text-turquoise disabled:opacity-40"
                                    disabled={held.current}
                                    title={
                                        held.current
                                            ? "this is the page in force"
                                            : "save this page again, so it becomes the rules and the newest save"
                                    }
                                    onClick={() => void put_back(held)}
                                >
                                    put back
                                </button>
                                <button
                                    className="rounded border border-reef px-1.5 font-mono text-[10px] text-shell hover:border-coral hover:text-coral"
                                    title="forget this save — the rules in force are not touched"
                                    onClick={() => void forget(held)}
                                >
                                    forget
                                </button>
                            </span>
                        </div>

                        {showing?.id === held.id ? (
                            <pre className="max-h-56 overflow-auto whitespace-pre-wrap break-words rounded border border-reef bg-lagoon-deep p-2 font-mono text-[10px] leading-relaxed text-driftwood">
                                {showing.text || "nothing at all"}
                            </pre>
                        ) : null}
                    </article>
                ))}
            </div>
        </section>
    );
}
