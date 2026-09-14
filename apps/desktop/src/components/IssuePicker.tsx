import { useEffect, useRef, useState } from "react";

import { card_from_issue, list_issues, type GitHubIssue, type Repository, type Task } from "@/lib/core";
import { issue_line, on_github, with_card } from "@/lib/issues";

const message_of = (cause: unknown) => (cause instanceof Error ? cause.message : String(cause));

/// The open GitHub issues of the projects on the board, each one a card away.
///
/// Work gets asked for in the issues while nobody is looking at the board, so
/// the board reaches into them rather than somebody copying titles across. A
/// card keeps the issue it came from, and its pull request closes it.
export function IssuePicker({ repos, on_made }: { repos: Repository[]; on_made: (task: Task) => void }) {
    const [open, set_open] = useState(false);
    const [read, set_read] = useState<Record<string, GitHubIssue[] | string>>({});
    const [making, set_making] = useState<string | null>(null);
    const [said, set_said] = useState<string | null>(null);
    const holder = useRef<HTMLDivElement>(null);
    const github = repos.filter(on_github);
    const shown = github.map((repository) => repository.id).join(",");

    useEffect(() => {
        if (!open) {
            return;
        }
        set_said(null);
        for (const repository of github) {
            list_issues(repository.id)
                .then((issues) => set_read((held) => ({ ...held, [repository.id]: issues })))
                .catch((cause) => set_read((held) => ({ ...held, [repository.id]: message_of(cause) })));
        }
        // Read once each time the list opens, for the projects on the board then.
    }, [open, shown]);

    useEffect(() => {
        if (!open) {
            return;
        }
        const dismiss = (event: MouseEvent) => {
            if (!holder.current?.contains(event.target as Node)) {
                set_open(false);
            }
        };
        window.addEventListener("mousedown", dismiss);
        return () => window.removeEventListener("mousedown", dismiss);
    }, [open]);

    if (github.length === 0) {
        return null;
    }

    const make = (repository_id: string, issue: GitHubIssue) => {
        set_making(`${repository_id}#${issue.number}`);
        set_said(null);
        card_from_issue(repository_id, issue.number)
            .then((task) => {
                set_read((held) => {
                    const issues = held[repository_id];
                    return Array.isArray(issues) ? { ...held, [repository_id]: with_card(issues, issue.number, task.id) } : held;
                });
                on_made(task);
            })
            .catch((cause) => set_said(message_of(cause)))
            .finally(() => set_making(null));
    };

    return (
        <div ref={holder} className="relative shrink-0">
            <button
                className="rounded-md border border-reef px-2 py-1 font-mono text-[11px] text-shell hover:border-turquoise hover:text-linen"
                title="make a card out of one of the projects' open GitHub issues"
                onClick={() => set_open(!open)}
            >
                from GitHub
            </button>

            {open ? (
                <div
                    data-issues
                    className="absolute left-0 top-full z-40 mt-1 max-h-[60vh] w-[28rem] max-w-[80vw] overflow-y-auto rounded-lg border border-foam bg-lagoon py-1 shadow-lg"
                >
                    {said ? <p className="px-3 py-1 font-mono text-[10px] text-coral">{said}</p> : null}

                    {github.map((repository) => {
                        const held = read[repository.id];
                        return (
                            <section key={repository.id} className="px-3 py-1.5">
                                <h3 className="font-mono text-[9px] uppercase tracking-[0.14em] text-shade">
                                    {repository.name}
                                </h3>
                                {held === undefined ? (
                                    <p className="font-mono text-[10px] text-shade">reading its open issues…</p>
                                ) : typeof held === "string" ? (
                                    <p className="font-mono text-[10px] text-coral">{held}</p>
                                ) : held.length === 0 ? (
                                    <p className="font-mono text-[10px] text-shade">no open issues</p>
                                ) : (
                                    held.map((issue) => (
                                        <div
                                            key={issue.number}
                                            data-issue={issue.number}
                                            className="flex items-start justify-between gap-2 py-1"
                                        >
                                            <div className="min-w-0">
                                                <div className="text-[11px] text-linen">
                                                    <span className="font-mono text-shade">#{issue.number}</span> {issue.title}
                                                </div>
                                                <div className="truncate font-mono text-[9px] text-shade">{issue_line(issue)}</div>
                                            </div>
                                            {issue.card ? (
                                                <span className="shrink-0 font-mono text-[10px] text-palm" title="the card made from it">
                                                    {issue.card}
                                                </span>
                                            ) : (
                                                <button
                                                    className="shrink-0 rounded-lg border border-turquoise px-1.5 font-mono text-[10px] text-turquoise disabled:opacity-40"
                                                    disabled={making !== null}
                                                    onClick={() => make(repository.id, issue)}
                                                >
                                                    {making === `${repository.id}#${issue.number}` ? "making…" : "make a card"}
                                                </button>
                                            )}
                                        </div>
                                    ))
                                )}
                            </section>
                        );
                    })}
                </div>
            ) : null}
        </div>
    );
}
