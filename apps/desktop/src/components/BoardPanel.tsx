import { useCallback, useEffect, useState } from "react";

import {
    assign_task,
    create_task,
    delete_task,
    list_agents,
    list_repos,
    list_tasks,
    move_task,
    open_pull_request,
    review_worktree,
    type Agent,
    type Column,
    type Repository,
    type Review,
    type Task,
} from "@/lib/core";

const COLUMNS: Column[] = ["backlog", "assigned", "working", "review", "done"];

function patch_line_color(line: string): string {
    if (line.startsWith("+++") || line.startsWith("---") || line.startsWith("diff ")) {
        return "text-[#7b8d94]";
    }
    if (line.startsWith("+")) {
        return "text-[#5aa87c]";
    }
    if (line.startsWith("-")) {
        return "text-[#d46969]";
    }
    if (line.startsWith("@@")) {
        return "text-[#45bcc4]";
    }
    return "text-[#a4b5bb]";
}

export function BoardPanel() {
    const [tasks, set_tasks] = useState<Task[]>([]);
    const [agents, set_agents] = useState<Agent[]>([]);
    const [repos, set_repos] = useState<Repository[]>([]);
    const [draft, set_draft] = useState({ title: "", body: "", repository_id: "" });
    const [review, set_review] = useState<{ task: Task; data: Review } | null>(null);
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

    useEffect(() => {
        refresh().catch((cause) => set_error(String(cause)));
        const handle = window.setInterval(() => {
            list_tasks().then(set_tasks).catch(() => undefined);
        }, 4000);
        return () => window.clearInterval(handle);
    }, [refresh]);

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
        <div className="flex min-h-0 flex-1">
            <div className="flex min-h-0 flex-1 flex-col gap-3 p-4">
                <div className="flex flex-wrap items-center gap-2">
                    <input
                        className="w-72 border border-[#26343a] bg-[#141c1f] px-2 py-1 font-mono text-xs"
                        placeholder="task title"
                        value={draft.title}
                        onChange={(event) => set_draft({ ...draft, title: event.target.value })}
                    />
                    <input
                        className="w-96 border border-[#26343a] bg-[#141c1f] px-2 py-1 font-mono text-xs"
                        placeholder="brief for the agent"
                        value={draft.body}
                        onChange={(event) => set_draft({ ...draft, body: event.target.value })}
                    />
                    <select
                        className="border border-[#26343a] bg-[#141c1f] px-2 py-1 font-mono text-xs"
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
                        className="border border-[#45bcc4] px-3 py-1 font-mono text-xs text-[#45bcc4] disabled:opacity-40"
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
                    <div className="border border-[#d46969] bg-[#1b1113] px-3 py-2 font-mono text-xs text-[#d46969]">
                        {error}
                    </div>
                ) : null}

                <div className="grid min-h-0 flex-1 grid-cols-5 gap-2">
                    {COLUMNS.map((column) => (
                        <div
                            key={column}
                            className="flex min-h-0 flex-col border border-[#26343a] bg-[#101719]"
                            onDragOver={(event) => event.preventDefault()}
                            onDrop={(event) => {
                                const id = event.dataTransfer.getData("text/plain");
                                if (id) {
                                    void run(() => move_task(id, column));
                                }
                            }}
                        >
                            <header className="border-b border-[#26343a] px-2 py-1 font-mono text-[11px] uppercase tracking-[0.1em] text-[#7b8d94]">
                                {column} · {tasks.filter((task) => task.column === column).length}
                            </header>

                            <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto p-2">
                                {tasks
                                    .filter((task) => task.column === column)
                                    .map((task) => (
                                        <article
                                            key={task.id}
                                            draggable
                                            onDragStart={(event) =>
                                                event.dataTransfer.setData("text/plain", task.id)
                                            }
                                            className="cursor-grab border border-[#26343a] bg-[#141c1f] p-2"
                                        >
                                            <div className="flex items-baseline justify-between gap-2">
                                                <span className="text-xs text-[#e3ebee]">{task.title}</span>
                                                <span className="font-mono text-[10px] text-[#5d6e75]">
                                                    {task.id}
                                                </span>
                                            </div>

                                            {task.branch ? (
                                                <div className="mt-1 font-mono text-[10px] text-[#45bcc4]">
                                                    {task.branch}
                                                </div>
                                            ) : null}

                                            {task.evidence.length > 0 ? (
                                                <div className="mt-1 font-mono text-[10px] text-[#5aa87c]">
                                                    {task.evidence.length} evidence
                                                </div>
                                            ) : null}

                                            <div className="mt-2 flex flex-wrap gap-1">
                                                {task.assignee ? (
                                                    <span className="border border-[#26343a] px-1 font-mono text-[10px] text-[#a4b5bb]">
                                                        {task.assignee}
                                                    </span>
                                                ) : (
                                                    <select
                                                        className="border border-[#26343a] bg-[#0d1315] px-1 font-mono text-[10px]"
                                                        value=""
                                                        onChange={(event) =>
                                                            run(() => assign_task(task.id, event.target.value))
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
                                                        className="border border-[#26343a] px-1 font-mono text-[10px] text-[#a4b5bb]"
                                                        onClick={() => void open_review(task)}
                                                    >
                                                        review
                                                    </button>
                                                ) : null}

                                                <button
                                                    className="border border-[#26343a] px-1 font-mono text-[10px] text-[#7b8d94]"
                                                    onClick={() => run(() => delete_task(task.id))}
                                                >
                                                    delete
                                                </button>
                                            </div>
                                        </article>
                                    ))}
                            </div>
                        </div>
                    ))}
                </div>
            </div>

            {review ? (
                <aside className="flex w-[46%] min-w-[440px] flex-col border-l border-[#26343a]">
                    <header className="flex items-center justify-between gap-2 border-b border-[#26343a] px-3 py-2">
                        <div className="font-mono text-[11px] text-[#7b8d94]">
                            {review.data.branch} vs {review.data.base} · {review.data.files} files ·{" "}
                            <span className="text-[#5aa87c]">+{review.data.insertions}</span>{" "}
                            <span className="text-[#d46969]">-{review.data.deletions}</span>
                            {review.data.uncommitted ? " · uncommitted work" : ""}
                        </div>
                        <div className="flex gap-2">
                            <button
                                className="border border-[#45bcc4] px-2 py-1 font-mono text-[11px] text-[#45bcc4] disabled:opacity-40"
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
                                className="border border-[#3a4d55] px-2 py-1 font-mono text-[11px]"
                                onClick={() => set_review(null)}
                            >
                                close
                            </button>
                        </div>
                    </header>

                    {review.data.commits.length > 0 ? (
                        <div className="border-b border-[#26343a] px-3 py-2 font-mono text-[11px] text-[#a4b5bb]">
                            {review.data.commits.map((commit) => (
                                <div key={commit.sha}>
                                    <span className="text-[#45bcc4]">{commit.sha}</span> {commit.subject}
                                </div>
                            ))}
                        </div>
                    ) : null}

                    <pre className="min-h-0 flex-1 overflow-auto p-3 font-mono text-[11px] leading-relaxed">
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
