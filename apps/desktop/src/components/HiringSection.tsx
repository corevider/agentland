import { useCallback, useEffect, useState } from "react";

import { Waiting } from "@/components/Spinner";
import { read_hiring, set_hiring, type HiringReport, type HiringRules } from "@/lib/core";
import { login_key, login_words } from "@/lib/hiring_rules";

function without(list: string[], key: string, open: boolean): string[] {
    return open ? list.filter((held) => held !== key) : [...new Set([...list, key])];
}

/// Which engines and logins the crew may hire onto, and what each engine is for.
///
/// The chief and the commanders read this every time they hire, with how much of
/// each login's week is left. Nothing is closed until somebody closes it, so a
/// new install or a new login is usable at once.
export function HiringSection() {
    const [held, set_held] = useState<HiringReport | null>(null);
    const [notes, set_notes] = useState<Record<string, string>>({});
    const [notice, set_notice] = useState<string | null>(null);

    const take = useCallback((report: HiringReport) => {
        set_held(report);
        set_notes(report.rules.notes);
    }, []);

    useEffect(() => {
        read_hiring()
            .then(take)
            .catch((cause) => set_notice(String(cause)));
    }, [take]);

    const change = useCallback(
        (rules: HiringRules) => {
            set_hiring(rules)
                .then((report) => {
                    take(report);
                    set_notice(null);
                })
                .catch((cause) => set_notice(cause instanceof Error ? cause.message : String(cause)));
        },
        [take],
    );

    if (!held) {
        return <Waiting says="asking the engines who they are…" className="font-mono text-[11px] text-shade" />;
    }

    const { rules } = held;
    const installed = held.engines.filter((engine) => engine.installed);

    return (
        <section className="flex flex-col gap-3">
            <p className="font-mono text-[11px] text-shade">
                What the chief and the commanders may hire onto. They read it every time they hire, with
                what you wrote about each engine and how much of each login's week is left; anything closed
                here is refused, for them and for the pickers in this window.
            </p>

            {installed.map((engine) => (
                <div
                    key={engine.id}
                    className="flex flex-col gap-2 rounded-lg border border-reef bg-lagoon-deep px-2 py-2"
                >
                    <label className="flex items-center gap-2">
                        <input
                            type="checkbox"
                            checked={engine.open}
                            onChange={(event) =>
                                change({
                                    ...rules,
                                    closed_engines: without(rules.closed_engines, engine.id, event.target.checked),
                                })
                            }
                        />
                        <span className={`font-mono text-[11px] ${engine.open ? "text-linen" : "text-shade"}`}>
                            {engine.name}
                        </span>
                        {engine.takes_the_tools ? null : (
                            <span className="font-mono text-[10px] text-sun">
                                works in its pane, but cannot use the crew's tools
                            </span>
                        )}
                    </label>

                    <input
                        className="rounded-lg border border-reef bg-lagoon px-2 py-1 font-mono text-[11px] disabled:opacity-50"
                        placeholder="what it is for — e.g. implementers and tests, not reviews"
                        value={notes[engine.id] ?? ""}
                        disabled={!engine.open}
                        onChange={(event) => set_notes({ ...notes, [engine.id]: event.target.value })}
                        onBlur={() => {
                            const written = notes[engine.id] ?? "";
                            if (written !== (rules.notes[engine.id] ?? "")) {
                                change({ ...rules, notes: { ...rules.notes, [engine.id]: written } });
                            }
                        }}
                    />

                    <div className="flex flex-col gap-1 pl-5">
                        {engine.logins.map((login) => {
                            const key = login_key(engine.id, login.account);
                            return (
                                <label key={key} className="flex items-center gap-2">
                                    <input
                                        type="checkbox"
                                        checked={login.open}
                                        disabled={!engine.open}
                                        onChange={(event) =>
                                            change({
                                                ...rules,
                                                closed_logins: without(rules.closed_logins, key, event.target.checked),
                                            })
                                        }
                                    />
                                    <span className="font-mono text-[11px] text-linen">
                                        {login.account ?? "this machine's login"}
                                    </span>
                                    <span className="font-mono text-[10px] text-shade">{login_words(login)}</span>
                                </label>
                            );
                        })}
                    </div>
                </div>
            ))}

            {notice ? <p className="font-mono text-[11px] text-sun">{notice}</p> : null}
        </section>
    );
}
