import { use_poll } from "@/lib/poll";

import { exactly, when } from "@/lib/when";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";

import { use_sideways_wheel } from "@/lib/wheel";

import {
    assign_task,
    create_task,
    delete_task,
    list_agents,
    list_repos,
    list_tasks,
    merge_worktree,
    move_task,
    open_pull_request,
    review_worktree,
    type Agent,
    type Column,
    type Entry,
    type Evidence,
    type Repository,
    type Review,
    type Task,
} from "@/lib/core";

const COLUMNS: Column[] = ["backlog", "assigned", "working", "review", "ready", "done"];

function patch_line_color(line: string): string {
    if (line.startsWith("+++") || line.startsWith("---") || line.startsWith("diff ")) {
        return "text-shell";
    }
    if (line.startsWith("+")) {
        return "text-palm";
    }
    if (line.startsWith("-")) {
        return "text-coral";
    }
    if (line.startsWith("@@")) {
        return "text-turquoise";
    }
    return "text-driftwood";
}

export function BoardPanel({ active, repositories }: { active: boolean; repositories: string[] | null }) {
    const columns = use_sideways_wheel<HTMLDivElement>();
    const [all_tasks, set_tasks] = useState<Task[]>([]);
    const tasks = repositories
        ? all_tasks.filter((task) => repositories.includes(task.repository_id))
        : all_tasks;
    const [agents, set_agents] = useState<Agent[]>([]);
    const [repos, set_repos] = useState<Repository[]>([]);
    const [draft, set_draft] = useState({ title: "", body: "", repository_id: "" });
    const [review, set_review] = useState<{ task: Task; data: Review } | null>(null);
    // The id rather than the card: the board polls, and a card held by value
    // would stop changing the moment it was opened.
    const [opened, set_opened] = useState<string | null>(null);
    const [error, set_error] = useState<string | null>(null);
    const [busy, set_busy] = useState(false);

    const refresh = useCallback(async () => {
        const [board, crew, repositories] = await Promise.all([
            list_tasks(),
            list_agents(),
            list_repos(),
        ]);
        set_tasks(board);
        set_agents(crew);
        set_repos(repositories);
        set_draft((current) => ({
            ...current,
            repository_id: current.repository_id || repositories[0]?.id || "",
        }));
    }, []);

    use_poll(() => {
        list_tasks().then(set_tasks).catch(() => undefined);
    }, 4000, active);

    const run = useCallback(
        async (action: () => Promise<unknown>) => {
            set_busy(true);
            set_error(null);
            try {
                await action();
                await refresh();
            } catch (cause) {
                set_error(cause instanceof Error ? cause.message : String(cause));
            } finally {
                set_busy(false);
            }
        },
        [refresh],
    );

    const open_review = useCallback(async (task: Task) => {
        if (!task.worktree) {
            set_error(`${task.id} has no worktree yet — assign it first`);
            return;
        }
        set_error(null);
        const data = await review_worktree(task.repository_id, task.worktree);
        set_review({ task, data });
    }, []);

    return (
        <div className="flex h-full min-h-0 min-w-0 flex-1">
            <div className="flex h-full min-h-0 min-w-0 flex-1 flex-col gap-3 p-2.5">
                <div className="flex flex-wrap items-center gap-2">
                    <input
                        className="min-w-[110px] flex-1 rounded-md border border-reef bg-lagoon px-2 py-1 font-mono text-[11px]"
                        placeholder="task title"
                        value={draft.title}
                        onChange={(event) => set_draft({ ...draft, title: event.target.value })}
                    />
                    <input
                        className="min-w-[130px] flex-[2] rounded-md border border-reef bg-lagoon px-2 py-1 font-mono text-[11px]"
                        placeholder="brief for the agent"
                        value={draft.body}
                        onChange={(event) => set_draft({ ...draft, body: event.target.value })}
                    />
                    <select
                        className="border border-reef bg-lagoon px-2 py-1 font-mono text-[11px] rounded-lg"
                        value={draft.repository_id}
                        onChange={(event) => set_draft({ ...draft, repository_id: event.target.value })}
                    >
                        {repos.map((repo) => (
                            <option key={repo.id} value={repo.id}>
                                {repo.name}
                            </option>
                        ))}
                    </select>
                    <button
                        className="shrink-0 rounded-md border border-turquoise px-2 py-1 font-mono text-[11px] text-turquoise disabled:opacity-40"
                        disabled={busy || !draft.title.trim() || !draft.repository_id}
                        onClick={() =>
                            run(async () => {
                                await create_task(draft.title.trim(), draft.body, draft.repository_id);
                                set_draft({ ...draft, title: "", body: "" });
                            })
                        }
                    >
                        add task
                    </button>
                </div>

                {error ? (
                    <div className="border border-coral bg-lagoon px-2 py-1 font-mono text-[11px] text-coral rounded-lg">
                        {error}
                    </div>
                ) : null}

                <div ref={columns} className="flex min-h-0 flex-1 gap-1.5 overflow-x-auto">
                    {COLUMNS.map((column) => (
                        <div
                            key={column}
                            className="flex min-h-0 w-[150px] shrink-0 flex-col rounded-md border border-reef bg-lagoon"
                            onDragOver={(event) => event.preventDefault()}
                            onDrop={(event) => {
                                const id = event.dataTransfer.getData("text/plain");
                                if (id) {
                                    void run(() => move_task(id, column));
                                }
                            }}
                        >
                            <header className="border-b border-reef px-2 py-1 font-mono text-[11px] uppercase tracking-[0.1em] text-shell">
                                {column} · {tasks.filter((task) => task.column === column).length}
                            </header>

                            <Column
                                tasks={tasks.filter((task) => task.column === column)}
                                render={(task) => (
                                    <BoardCard
                                        key={task.id}
                                        task={task}
                                        agents={agents}
                                        on_open={() => set_opened(task.id)}
                                        on_assign={(agent_id) => run(() => assign_task(task.id, agent_id))}
                                        on_review={() => void open_review(task)}
                                        on_delete={() => run(() => delete_task(task.id))}
                                    />
                                )}
                            />
                        </div>
                    ))}
                </div>
            </div>

            {!review && opened && tasks.some((task) => task.id === opened) ? (
                <CardDetail
                    task={tasks.find((task) => task.id === opened)!}
                    on_close={() => set_opened(null)}
                    on_review={() => {
                        const held = tasks.find((task) => task.id === opened);
                        if (held) {
                            void open_review(held);
                        }
                    }}
                    on_merge={() => {
                        const held = tasks.find((task) => task.id === opened);
                        if (held?.worktree) {
                            void run(() =>
                                merge_worktree(held.repository_id, held.worktree!, held.id),
                            );
                        }
                    }}
                />
            ) : null}

            {review ? (
                <aside className="flex w-[46%] min-w-[440px] flex-col border-l border-reef">
                    <header className="flex items-center justify-between gap-2 border-b border-reef px-2 py-1">
                        <div className="font-mono text-[11px] text-shell">
                            {review.data.branch} vs {review.data.base} · {review.data.files} files ·{" "}
                            <span className="text-palm">+{review.data.insertions}</span>{" "}
                            <span className="text-coral">-{review.data.deletions}</span>
                            {review.data.uncommitted ? " · uncommitted work" : ""}
                        </div>
                        <div className="flex gap-2">
                            <button
                                className="border border-turquoise px-2 py-1 font-mono text-[11px] text-turquoise disabled:opacity-40 rounded-lg"
                                disabled={busy}
                                onClick={() =>
                                    run(async () => {
                                        const result = await open_pull_request(
                                            review.task.repository_id,
                                            review.task.worktree as string,
                                            review.task.title,
                                            review.task.body,
                                            review.task.id,
                                        );
                                        set_error(`${result.detail}: ${result.url}`);
                                    })
                                }
                            >
                                open pull request
                            </button>
                            <button
                                className="border border-foam px-2 py-1 font-mono text-[11px] rounded-lg"
                                onClick={() => set_review(null)}
                            >
                                close
                            </button>
                        </div>
                    </header>

                    {review.data.commits.length > 0 ? (
                        <div className="border-b border-reef px-2 py-1 font-mono text-[11px] text-driftwood">
                            {review.data.commits.map((commit) => (
                                <div key={commit.sha}>
                                    <span className="text-turquoise">{commit.sha}</span> {commit.subject}
                                </div>
                            ))}
                        </div>
                    ) : null}

                    <pre className="min-h-0 flex-1 overflow-auto p-2 font-mono text-[11px] leading-relaxed">
                        {review.data.patch.split("\n").map((line, index) => (
                            <div key={index} className={patch_line_color(line)}>
                                {line || " "}
                            </div>
                        ))}
                    </pre>
                </aside>
            ) : null}
        </div>
    );
}

/// A column that draws only the cards in view.
///
/// Measured on a board of 325: every card in the DOM cost 12 fps with the panel
/// on screen. Only what fits is rendered, plus a few rows either side so a
/// scroll never shows a gap.
function Column({ tasks, render }: { tasks: Task[]; render: (task: Task) => ReactNode }) {
    const holder = useRef<HTMLDivElement>(null);

    const rows = useVirtualizer({
        count: tasks.length,
        getScrollElement: () => holder.current,
        estimateSize: () => 96,
        overscan: 6,
    });

    return (
        <div ref={holder} className="min-h-0 flex-1 overflow-y-auto p-2">
            <div className="relative w-full" style={{ height: rows.getTotalSize() }}>
                {rows.getVirtualItems().map((row) => (
                    <div
                        key={tasks[row.index].id}
                        ref={rows.measureElement}
                        data-index={row.index}
                        className="absolute inset-x-0 pb-2"
                        style={{ transform: `translateY(${row.start}px)` }}
                    >
                        {render(tasks[row.index])}
                    </div>
                ))}
            </div>
        </div>
    );
}

const KIND_TINT: Record<string, string> = {
    finished: "text-palm",
    commit: "text-turquoise",
    diff: "text-shell",
    pull_request: "text-sun",
    note: "text-shade",
};

/// The evidence inside an entry, whichever shape it arrived in.
///
/// A core that has not been restarted since the board learned to record who did
/// what still serves bare evidence with no `what` around it. The window and the
/// core are separate processes and one can be older than the other, so the
/// reader takes both rather than showing an empty history for a version skew.
function what_of(entry: Entry): Evidence {
    return entry.what ?? (entry as unknown as Evidence);
}

/// One line of a card's history, in the words of whoever wrote it.
function said(entry: Entry): string {
    const what = what_of(entry);
    switch (what.kind) {
        case "commit":
            return `${String(what.sha).slice(0, 7)} ${what.subject}`;
        case "diff":
            return `${what.files} files · +${what.insertions} −${what.deletions}`;
        case "pull_request":
            return String(what.url);
        case "finished": {
            const touched = Number(what.files ?? 0);
            const size = touched
                ? ` · ${touched} file${touched === 1 ? "" : "s"} +${what.insertions} −${what.deletions}`
                : "";
            return `${what.summary}${size}`;
        }
        default:
            return String(what.text ?? what.kind);
    }
}

/// Everything the card knows about itself: what was asked, who took it, where
/// they worked, and what each of them left behind.
///
/// A card used to say "3 evidence" and nothing more, which is the count of an
/// answer rather than the answer.
function CardDetail({
    task,
    on_close,
    on_review,
    on_merge,
}: {
    task: Task;
    on_close: () => void;
    on_review: () => void;
    on_merge: () => void;
}) {
    const now = Math.floor(Date.now() / 1000);
    const finish = task.evidence.filter((entry) => what_of(entry).kind === "finished").at(-1);

    return (
        <aside className="flex w-[46%] min-w-[380px] flex-col border-l border-reef">
            <header className="flex items-start justify-between gap-2 border-b border-reef px-2 py-1.5">
                <div className="min-w-0">
                    <div className="text-[12px] text-linen">{task.title}</div>
                    <div className="font-mono text-[10px] text-shade" title={exactly(task.at ?? 0)}>
                        {task.id} · {task.column} · written {when(task.at ?? 0, now)}
                    </div>
                </div>
                <button
                    className="rounded px-1.5 font-mono text-[11px] text-shell hover:text-linen"
                    onClick={on_close}
                >
                    ✕
                </button>
            </header>

            <div className="flex min-h-0 flex-1 flex-col gap-2.5 overflow-y-auto p-2.5">
                {task.body.trim() ? (
                    <p className="whitespace-pre-wrap text-[11px] text-shell">{task.body}</p>
                ) : null}

                <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 font-mono text-[10px]">
                    <dt className="text-shade">who</dt>
                    <dd className="text-linen">{task.assignee ?? "nobody yet"}</dd>
                    <dt className="text-shade">where</dt>
                    <dd className="text-linen">{task.worktree ?? "not bound to a worktree"}</dd>
                    <dt className="text-shade">branch</dt>
                    <dd className="text-turquoise">{task.branch ?? "none yet"}</dd>
                    <dt className="text-shade">project</dt>
                    <dd className="text-linen">{task.repository_id}</dd>
                </dl>

                {finish ? (
                    <section className="rounded-md border border-palm/60 bg-lagoon-deep px-2 py-1.5">
                        <h3 className="font-mono text-[9px] uppercase tracking-[0.14em] text-palm">
                            How it ended
                        </h3>
                        <p className="mt-0.5 text-[11px] text-linen">{said(finish)}</p>
                        <p className="font-mono text-[10px] text-shade" title={exactly(finish.at)}>
                            {finish.by ?? "someone"} · {finish.at ? when(finish.at, now) : "no date"}
                        </p>
                    </section>
                ) : null}

                <section>
                    <h3 className="mb-1 font-mono text-[9px] uppercase tracking-[0.14em] text-shade">
                        What happened · {task.evidence.length}
                    </h3>

                    {task.evidence.length === 0 ? (
                        <p className="font-mono text-[10px] text-shade">
                            Nothing has been recorded on this card yet.
                        </p>
                    ) : null}

                    <ol className="flex flex-col gap-1">
                        {task.evidence.map((entry, index) => (
                            <li
                                key={`${entry.at}-${index}`}
                                className="rounded-md border border-reef bg-lagoon-deep px-2 py-1"
                            >
                                <div className={`text-[11px] ${KIND_TINT[what_of(entry).kind] ?? "text-shell"}`}>
                                    {said(entry)}
                                </div>
                                <div
                                    className="font-mono text-[10px] text-shade"
                                    title={entry.at ? exactly(entry.at) : "before this was recorded"}
                                >
                                    {what_of(entry).kind} · {entry.by ?? "someone"} ·{" "}
                                    {entry.at ? when(entry.at, now) : "no date"}
                                </div>
                            </li>
                        ))}
                    </ol>
                </section>

                <div className="flex flex-wrap gap-2">
                    {task.worktree ? (
                        <button
                            className="rounded-lg border border-turquoise px-2 py-0.5 font-mono text-[11px] text-turquoise"
                            onClick={on_review}
                        >
                            read the diff
                        </button>
                    ) : null}

                    {task.column === "ready" && task.worktree ? (
                        <button
                            className="rounded-lg border border-palm px-2 py-0.5 font-mono text-[11px] text-palm"
                            onClick={on_merge}
                            title="squash and merge the pull request, and finish this card"
                        >
                            merge it
                        </button>
                    ) : null}
                </div>
            </div>
        </aside>
    );
}

function BoardCard({
    task,
    agents,
    on_open,
    on_assign,
    on_review,
    on_delete,
}: {
    task: Task;
    agents: Agent[];
    on_open: () => void;
    on_assign: (agent_id: string) => void;
    on_review: () => void;
    on_delete: () => void;
}) {
    return (
        <article
                            key={task.id}
                            draggable
                            onDragStart={(event) =>
                                event.dataTransfer.setData("text/plain", task.id)
                            }
                            onClick={on_open}
                            className="cursor-grab border border-reef bg-lagoon p-2 rounded-lg"
                        >
                            <div className="flex items-baseline justify-between gap-2">
                                <span className="text-[11px] text-linen">{task.title}</span>
                                <span className="font-mono text-[10px] text-shade" title={exactly(task.at ?? 0)}>
                                    {task.id} · {when(task.at ?? 0, Math.floor(Date.now() / 1000))}
                                </span>
                            </div>
        
                            {task.branch ? (
                                <div className="mt-1 font-mono text-[10px] text-turquoise">
                                    {task.branch}
                                </div>
                            ) : null}
        
                            {task.evidence.length > 0 ? (
                                <div className="mt-1 font-mono text-[10px] text-palm">
                                    {task.evidence.some((entry) => what_of(entry).kind === "finished")
                                        ? "finished · "
                                        : ""}
                                    {task.evidence.length} on its history
                                </div>
                            ) : null}
        
                            <div className="mt-2 flex flex-wrap gap-1">
                                {task.assignee ? (
                                    <span className="border border-reef px-1 font-mono text-[10px] text-driftwood rounded-lg">
                                        {task.assignee}
                                    </span>
                                ) : (
                                    <select
                                        className="border border-reef bg-lagoon-deep px-1 font-mono text-[10px] rounded-lg"
                                        value=""
                                        onChange={(event) =>
                                            on_assign(event.target.value)
                                        }
                                    >
                                        <option value="">assign…</option>
                                        {agents
                                            .filter(
                                                (agent) =>
                                                    agent.repository_id === task.repository_id,
                                            )
                                            .map((agent) => (
                                                <option key={agent.id} value={agent.id}>
                                                    {agent.name}
                                                </option>
                                            ))}
                                    </select>
                                )}
        
                                {task.worktree ? (
                                    <button
                                        className="border border-reef px-1 font-mono text-[10px] text-driftwood rounded-lg"
                                        onClick={() => on_review()}
                                    >
                                        review
                                    </button>
                                ) : null}
        
                                <button
                                    className="border border-reef px-1 font-mono text-[10px] text-shell rounded-lg"
                                    onClick={() => on_delete()}
                                >
                                    delete
                                </button>
                            </div>
                        </article>
    );
}
