import { useCallback, useEffect, useState } from "react";

import {
    add_repo,
    create_worktree,
    list_repos,
    list_services,
    list_worktrees,
    remove_worktree,
    start_service,
    stop_service,
    type Repository,
    type Service,
    type WorktreeStatus,
} from "@/lib/core";

const STATE_COLOR: Record<Service["state"], string> = {
    starting: "text-[#c99a2e]",
    ready: "text-[#5aa87c]",
    unreachable: "text-[#d46969]",
    stopped: "text-[#7b8d94]",
};

export function RepoPanel() {
    const [repos, set_repos] = useState<Repository[]>([]);
    const [worktrees, set_worktrees] = useState<Record<string, WorktreeStatus[]>>({});
    const [services, set_services] = useState<Record<string, Service>>({});
    const [preview, set_preview] = useState<string | null>(null);
    const [path, set_path] = useState("");
    const [names, set_names] = useState<Record<string, string>>({});
    const [error, set_error] = useState<string | null>(null);
    const [busy, set_busy] = useState(false);

    const refresh = useCallback(async () => {
        const current = await list_repos();
        set_repos(current);

        const entries = await Promise.all(
            current.map(async (repo) => [repo.id, await list_worktrees(repo.id)] as const),
        );
        set_worktrees(Object.fromEntries(entries));

        const running = await list_services();
        set_services(Object.fromEntries(running.map((service) => [service.key, service])));
    }, []);

    useEffect(() => {
        refresh().catch((cause) => set_error(String(cause)));
        const handle = window.setInterval(() => {
            refresh().catch(() => undefined);
        }, 3000);
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

    return (
        <div className="flex min-h-0 flex-1">
            <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-4">
                <div className="flex flex-wrap items-center gap-2">
                    <input
                        className="w-96 border border-[#26343a] bg-[#141c1f] px-2 py-1 font-mono text-xs"
                        placeholder="/path/to/a/git/repository"
                        value={path}
                        onChange={(event) => set_path(event.target.value)}
                    />
                    <button
                        className="border border-[#45bcc4] px-3 py-1 font-mono text-xs text-[#45bcc4] disabled:opacity-40"
                        disabled={busy || path.trim().length === 0}
                        onClick={() =>
                            run(async () => {
                                await add_repo(path.trim());
                                set_path("");
                            })
                        }
                    >
                        add repository
                    </button>
                </div>

                {error ? (
                    <div className="border border-[#d46969] bg-[#1b1113] px-3 py-2 font-mono text-xs text-[#d46969]">
                        {error}
                    </div>
                ) : null}

                {repos.length === 0 ? (
                    <p className="max-w-prose font-mono text-xs text-[#7b8d94]">
                        No repositories yet. Worktrees are created outside your clone, so nothing here
                        rearranges your folders.
                    </p>
                ) : null}

                {repos.map((repo) => (
                    <section key={repo.id} className="border border-[#26343a] bg-[#141c1f]">
                        <header className="flex flex-wrap items-baseline justify-between gap-3 border-b border-[#26343a] px-3 py-2">
                            <span className="font-mono text-sm text-[#e3ebee]">{repo.name}</span>
                            <span className="font-mono text-[11px] text-[#7b8d94]">
                                {repo.default_branch}
                                {repo.remotes.length > 0
                                    ? ` · ${repo.remotes.map((remote) => `${remote.name}@${remote.provider}`).join(", ")}`
                                    : " · no remote"}
                            </span>
                        </header>

                        <div className="flex flex-col gap-2 p-3">
                            {(worktrees[repo.id] ?? []).map((entry) => {
                                const key = `${repo.id}/${entry.name}`;
                                const service = services[key];

                                return (
                                    <div
                                        key={entry.name}
                                        className="flex flex-wrap items-center gap-3 border border-[#26343a] px-3 py-2 font-mono text-xs"
                                    >
                                        <span className="text-[#e3ebee]">{entry.name}</span>
                                        <span className="text-[#7b8d94]">{entry.branch}</span>
                                        <span className="text-[#45bcc4]">:{entry.port}</span>
                                        <span
                                            className={
                                                entry.dirty_files > 0 ? "text-[#c99a2e]" : "text-[#5aa87c]"
                                            }
                                        >
                                            {entry.missing
                                                ? "missing"
                                                : entry.dirty_files > 0
                                                  ? `${entry.dirty_files} uncommitted`
                                                  : "clean"}
                                        </span>

                                        {service ? (
                                            <span className={STATE_COLOR[service.state]} title={service.command}>
                                                server {service.state}
                                            </span>
                                        ) : null}

                                        <div className="ml-auto flex items-center gap-2">
                                            {service ? (
                                                <>
                                                    <button
                                                        className="border border-[#3a4d55] px-2 py-1 text-[11px] disabled:opacity-40"
                                                        disabled={service.state !== "ready"}
                                                        onClick={() =>
                                                            set_preview(preview === service.url ? null : service.url)
                                                        }
                                                    >
                                                        {preview === service.url ? "hide preview" : "preview"}
                                                    </button>
                                                    <button
                                                        className="border border-[#3a4d55] px-2 py-1 text-[11px] disabled:opacity-40"
                                                        disabled={busy}
                                                        onClick={() => run(() => stop_service(repo.id, entry.name))}
                                                    >
                                                        stop server
                                                    </button>
                                                </>
                                            ) : (
                                                <button
                                                    className="border border-[#3a4d55] px-2 py-1 text-[11px] disabled:opacity-40"
                                                    disabled={busy || entry.missing}
                                                    onClick={() => run(() => start_service(repo.id, entry.name))}
                                                >
                                                    start server
                                                </button>
                                            )}
                                            <button
                                                className="border border-[#3a4d55] px-2 py-1 text-[11px] disabled:opacity-40"
                                                disabled={busy}
                                                onClick={() => run(() => remove_worktree(repo.id, entry.name, false))}
                                            >
                                                remove
                                            </button>
                                            <button
                                                className="border border-[#9e3535] px-2 py-1 text-[11px] text-[#d46969] disabled:opacity-40"
                                                disabled={busy}
                                                onClick={() => run(() => remove_worktree(repo.id, entry.name, true))}
                                            >
                                                force
                                            </button>
                                        </div>
                                    </div>
                                );
                            })}

                            <div className="flex items-center gap-2">
                                <input
                                    className="w-48 border border-[#26343a] bg-[#0d1315] px-2 py-1 font-mono text-xs"
                                    placeholder="work1"
                                    value={names[repo.id] ?? ""}
                                    onChange={(event) =>
                                        set_names((current) => ({ ...current, [repo.id]: event.target.value }))
                                    }
                                />
                                <button
                                    className="border border-[#3a4d55] px-3 py-1 font-mono text-xs disabled:opacity-40"
                                    disabled={busy || !(names[repo.id] ?? "").trim()}
                                    onClick={() =>
                                        run(async () => {
                                            await create_worktree(repo.id, (names[repo.id] ?? "").trim());
                                            set_names((current) => ({ ...current, [repo.id]: "" }));
                                        })
                                    }
                                >
                                    create worktree
                                </button>
                            </div>
                        </div>
                    </section>
                ))}
            </div>

            {preview ? (
                <aside className="flex w-[46%] min-w-[420px] flex-col border-l border-[#26343a]">
                    <div className="flex items-center justify-between border-b border-[#26343a] px-3 py-2 font-mono text-[11px] text-[#7b8d94]">
                        <span>{preview}</span>
                        <button
                            className="border border-[#3a4d55] px-2 py-1"
                            onClick={() => set_preview(null)}
                        >
                            close
                        </button>
                    </div>
                    <iframe title="worktree preview" src={preview} className="min-h-0 flex-1 bg-white" />
                </aside>
            ) : null}
        </div>
    );
}
