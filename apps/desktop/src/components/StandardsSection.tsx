import { useCallback, useEffect, useState } from "react";

import { Waiting } from "@/components/Spinner";
import { read_standards, set_standards } from "@/lib/core";

/// How the house works.
///
/// Not a brief and not a memory: a brief is one piece of work and a memory is
/// something the crew learned. This is what holds for everybody, every turn —
/// naming, commit messages, what never goes in a log.
export function StandardsSection() {
    const [draft, set_draft] = useState<string | null>(null);
    const [held, set_held] = useState(false);
    const [notice, set_notice] = useState<string | null>(null);
    const [saving, set_saving] = useState(false);

    const refresh = useCallback(async () => {
        const rules = await read_standards();
        set_draft((current) => current ?? rules.text);
        set_held(rules.held);
    }, []);

    useEffect(() => {
        refresh().catch((cause) => set_notice(String(cause)));
    }, [refresh]);

    const save = useCallback(async () => {
        if (draft === null) {
            return;
        }

        set_saving(true);
        try {
            const rules = await set_standards(draft);
            set_held(rules.held);
            set_notice(null);
        } catch (cause) {
            set_notice(cause instanceof Error ? cause.message : String(cause));
        } finally {
            set_saving(false);
        }
    }, [draft]);

    if (draft === null) {
        return <Waiting says="reading the house rules…" className="font-mono text-[11px] text-shade" />;
    }

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
                className="min-h-0 flex-1 rounded-lg border border-reef bg-lagoon p-2 font-mono text-[11px] leading-relaxed"
                spellCheck={false}
                placeholder={"# House rules\n\n- Four spaces, and names that say what they are.\n- Tests with behaviour changes.\n- Never a secret in a log."}
                value={draft}
                onChange={(event) => set_draft(event.target.value)}
            />

            <div className="flex items-center gap-2">
                <button
                    className="rounded-lg border border-turquoise px-2 py-1 font-mono text-[11px] text-turquoise disabled:opacity-40"
                    disabled={saving}
                    onClick={() => void save()}
                >
                    {saving ? "saving…" : "save"}
                </button>
                <span className="font-mono text-[10px] text-shade">
                    {held
                        ? "in force — agents started from now on are handed it"
                        : "nothing set; agents are told only what their work needs"}
                </span>
            </div>

            {notice ? <p className="font-mono text-[11px] text-coral">{notice}</p> : null}
        </section>
    );
}
