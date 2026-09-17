import { ProjectOptions, SavedProjectOptions, empty_project_settings } from "@/components/ProjectOptions";
import { use_poll } from "@/lib/poll";
import { useCallback, useState } from "react";

import {
    add_repo,
    clone_repo,
    create_worktree,
    forget_repo,
    list_repos,
    list_services,
    list_worktrees,
    remove_worktree,
    start_service,
    stop_service,
    type Repository,
    type ProjectSettings,
    type Service,
    type WorktreeStatus,
} from "@/lib/core";
import { as_url, clone_target, is_clonable, pick_folder } from "@/lib/pick";
import { Press } from "@/components/Press";

const STATE_COLOR: Record<Service["state"], string> = {
    starting: "text-sun",
    ready: "text-palm",
    unreachable: "text-coral",
    stopped: "text-shell",
};

export function RepoPanel({ active }: { active: boolean }) {
    const [repos, set_repos] = useState<Repository[]>([]);
    const [worktrees, set_worktrees] = useState<Record<string, WorktreeStatus[]>>({});
    const [services, set_services] = useState<Record<string, Service>>({});
    const [preview, set_preview] = useState<string | null>(null);
    const [path, set_path] = useState("");
    const [settings, set_settings] = useState<ProjectSettings | undefined>();
    const [url, set_url] = useState("");
    const [into, set_into] = useState("");
    const [needs_git, set_needs_git] = useState<string | null>(null);
    const [names, set_names] = useState<Record<string, string>>({});
    const [error, set_error] = useState<string | null>(null);
    /// What has a change on its way, by what it is about: the open box, the
    /// clone box, a project, or one worktree. A worktree's remove and its
    /// server's stop would race each other; nothing about one worktree is a
    /// reason to hold up another.
    const [busy_on, set_busy_on] = useState<ReadonlySet<string>>(new Set());

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

    use_poll(() => { refresh().catch((cause) => set_error(String(cause))); }, 3000, active);

    const run = useCallback(
        async (key: string, action: () => Promise<unknown>) => {
            set_busy_on((held) => new Set(held).add(key));
            set_error(null);
            try {
                await action();
                await refresh();
            } catch (cause) {
                set_error(cause instanceof Error ? cause.message : String(cause));
            } finally {
                set_busy_on((held) => {
                    const next = new Set(held);
                    next.delete(key);
                    return next;
                });
            }
        },
        [refresh],
    );

    return (
        <div className="flex h-full min-h-0 min-w-0 flex-1">
            <div className="flex h-full min-h-0 min-w-0 flex-1 flex-col gap-2.5 overflow-y-auto p-2.5">
                <section className="flex flex-col gap-2 rounded-lg border border-reef bg-lagoon-deep p-2">
                    <h3 className="font-mono text-[9px] uppercase tracking-[0.14em] text-shade">
                        Open a project
                    </h3>

                    <div className="flex flex-wrap items-center gap-2">
                        <input
                            className="min-w-[18rem] flex-1 rounded-lg border border-reef bg-lagoon px-2 py-1 font-mono text-[11px]"
                            placeholder="a folder on this machine"
                            value={path}
                            onChange={(event) => {
                                set_path(event.target.value);
                                set_needs_git(null);
                            }}
                        />
                        <Press
                            className="rounded-lg border border-reef px-3 py-1 font-mono text-[11px] text-shell hover:border-foam"
                            disabled={busy_on.has("open")}
                            on_press={() =>
                                run("open", async () => {
                                    const chosen = await pick_folder("Open a project folder", path || undefined);
                                    if (chosen) {
                                        set_path(chosen);
                                        set_needs_git(null);
                                    }
                                })
                            }
                        >
                            browse…
                        </Press>
                        <Press
                            className="rounded-lg border border-turquoise px-3 py-1 font-mono text-[11px] text-turquoise disabled:opacity-40"
                            disabled={busy_on.has("open") || path.trim().length === 0}
                            busy_says="opening…"
                            on_press={() => {
                                const wanted = path.trim();
                                set_needs_git(null);
                                return run("open", async () => {
                                    try {
                                        await add_repo(wanted, false, settings);
                                        set_path("");
                                        set_settings(undefined);
                                    } catch (cause) {
                                        const said = cause instanceof Error ? cause.message : String(cause);
                                        // A folder that is not a repository yet is not a
                                        // mistake — it is the other half of the question.
                                        if (said.includes("not a git repository")) {
                                            set_needs_git(wanted);
                                            return;
                                        }
                                        throw cause;
                                    }
                                });
                            }}
                        >
                            open folder
                        </Press>
                    </div>

                    <ProjectOptions value={settings ?? empty_project_settings()} on_change={set_settings} disabled={busy_on.has("open") || busy_on.has("clone")} />

                    {needs_git ? (
                        <div className="flex flex-wrap items-center gap-2 rounded-lg border border-sun px-2 py-1">
                            <span className="font-mono text-[11px] text-sun">
                                {needs_git} is not a git repository yet. Each agent works in its own worktree,
                                which needs one.
                            </span>
                            <Press
                                className="rounded-lg border border-sun px-2 py-[3px] font-mono text-[11px] text-sun hover:bg-sun/10"
                                disabled={busy_on.has("open")}
                                busy_says="starting one…"
                                on_press={() =>
                                    run("open", async () => {
                                        await add_repo(needs_git, true, settings);
                                        set_needs_git(null);
                                        set_path("");
                                        set_settings(undefined);
                                    })
                                }
                            >
                                start one here
                            </Press>
                        </div>
                    ) : null}

                    <div className="flex flex-wrap items-center gap-2">
                        <input
                            className="min-w-[18rem] flex-1 rounded-lg border border-reef bg-lagoon px-2 py-1 font-mono text-[11px]"
                            placeholder="or clone: a git URL, or owner/repo"
                            value={url}
                            onChange={(event) => set_url(event.target.value)}
                        />
                        <Press
                            className="rounded-lg border border-reef px-3 py-1 font-mono text-[11px] text-shell hover:border-foam"
                            disabled={busy_on.has("clone")}
                            on_press={() =>
                                run("clone", async () => {
                                    const chosen = await pick_folder("Clone into…", into || undefined);
                                    if (chosen) {
                                        set_into(chosen);
                                    }
                                })
                            }
                        >
                            clone into…
                        </Press>
                        <Press
                            className="rounded-lg border border-turquoise px-3 py-1 font-mono text-[11px] text-turquoise disabled:opacity-40"
                            disabled={busy_on.has("clone") || !is_clonable(url) || into.trim().length === 0}
                            busy_says="cloning…"
                            on_press={() =>
                                run("clone", async () => {
                                    await clone_repo(as_url(url), into.trim(), settings);
                                    set_url("");
                                    set_settings(undefined);
                                })
                            }
                        >
                            clone
                        </Press>
                    </div>

                    {url.trim() && into.trim() ? (
                        <p className="font-mono text-[10px] text-shade">
                            lands in {clone_target(as_url(url), into.trim())}
                        </p>
                    ) : url.trim() ? (
                        <p className="font-mono text-[10px] text-shade">pick where it should land</p>
                    ) : null}
                </section>

                {error ? (
                    <div className="border border-coral bg-lagoon px-2 py-1 font-mono text-[11px] text-coral rounded-lg">
                        {error}
                    </div>
                ) : null}

                {repos.length === 0 ? (
                    <p className="max-w-prose font-mono text-[11px] text-shell">
                        No repositories yet. Worktrees are created outside your clone, so nothing here
                        rearranges your folders.
                    </p>
                ) : null}

                {repos.map((repo) => (
                    <section key={repo.id} className={`border bg-lagoon rounded-lg ${repo.missing ? "border-coral/70" : "border-reef"}`}>
                        <header className="flex flex-wrap items-baseline justify-between gap-3 border-b border-reef px-2 py-1">
                            <span className="font-mono text-[13px] text-linen">
                                {repo.name}
                                {repo.missing ? <span className="ml-2 text-[11px] text-coral">checkout gone</span> : null}
                            </span>
                            <span className="flex items-baseline gap-2 font-mono text-[11px] text-shell">
                                {repo.default_branch}
                                {repo.remotes.length > 0
                                    ? ` · ${repo.remotes.map((remote) => `${remote.name}@${remote.provider}`).join(", ")}`
                                    : " · no remote"}
                                <Press
                                    className={`rounded px-1 hover:text-coral ${repo.missing ? "border border-coral text-coral" : "text-shade"}`}
                                    title="stop tracking this project — the folder is left alone"
                                    disabled={busy_on.has(repo.id)}
                                    busy_says="forgetting…"
                                    on_press={() => run(repo.id, () => forget_repo(repo.id))}
                                >
                                    forget
                                </Press>
                            </span>
                        </header>

                        <SavedProjectOptions repository={repo} />

                        {repo.missing ? (
                            <p className="border-b border-coral/40 bg-coral/10 px-2 py-1 font-mono text-[11px] text-coral">
                                Its checkout is not on disk any more: {repo.primary_path}. Nothing can be opened
                                or cut from it. Add the project again from where it lives now, or forget this one.
                            </p>
                        ) : null}

                        <div className="flex flex-col gap-2 p-2">
                            {(worktrees[repo.id] ?? []).map((entry) => {
                                const key = `${repo.id}/${entry.name}`;
                                const service = services[key];

                                return (
                                    <div
                                        key={entry.name}
                                        className="flex flex-wrap items-center gap-3 border border-reef px-2 py-1 font-mono text-[11px] rounded-lg"
                                    >
                                        <span className="text-linen">{entry.name}</span>
                                        <span className="text-shell">{entry.branch}</span>
                                        <span className="text-turquoise">:{entry.port}</span>
                                        <span
                                            className={
                                                entry.dirty_files > 0 ? "text-sun" : "text-palm"
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
                                                        className="border border-foam px-2 py-1 text-[11px] disabled:opacity-40 rounded-lg"
                                                        disabled={service.state !== "ready"}
                                                        onClick={() =>
                                                            set_preview(preview === service.url ? null : service.url)
                                                        }
                                                    >
                                                        {preview === service.url ? "hide preview" : "preview"}
                                                    </button>
                                                    <Press
                                                        className="border border-foam px-2 py-1 text-[11px] disabled:opacity-40 rounded-lg"
                                                        disabled={busy_on.has(key)}
                                                        busy_says="stopping…"
                                                        on_press={() => run(key, () => stop_service(repo.id, entry.name))}
                                                    >
                                                        stop server
                                                    </Press>
                                                </>
                                            ) : (
                                                <Press
                                                    className="border border-foam px-2 py-1 text-[11px] disabled:opacity-40 rounded-lg"
                                                    disabled={busy_on.has(key) || entry.missing}
                                                    busy_says="starting…"
                                                    on_press={() => run(key, () => start_service(repo.id, entry.name))}
                                                >
                                                    start server
                                                </Press>
                                            )}
                                            <Press
                                                className="border border-foam px-2 py-1 text-[11px] disabled:opacity-40 rounded-lg"
                                                disabled={busy_on.has(key)}
                                                busy_says="removing…"
                                                on_press={() => run(key, () => remove_worktree(repo.id, entry.name, false))}
                                            >
                                                remove
                                            </Press>
                                            <Press
                                                className="border border-coral px-2 py-1 text-[11px] text-coral disabled:opacity-40 rounded-lg"
                                                disabled={busy_on.has(key)}
                                                busy_says="removing…"
                                                on_press={() => run(key, () => remove_worktree(repo.id, entry.name, true))}
                                            >
                                                force
                                            </Press>
                                        </div>
                                    </div>
                                );
                            })}

                            <div className="flex items-center gap-2">
                                <input
                                    className="w-48 border border-reef bg-lagoon-deep px-2 py-1 font-mono text-[11px] rounded-lg"
                                    placeholder="work1"
                                    value={names[repo.id] ?? ""}
                                    onChange={(event) =>
                                        set_names((current) => ({ ...current, [repo.id]: event.target.value }))
                                    }
                                />
                                <Press
                                    className="border border-foam px-3 py-1 font-mono text-[11px] disabled:opacity-40 rounded-lg"
                                    disabled={busy_on.has(repo.id) || !(names[repo.id] ?? "").trim()}
                                    busy_says="creating…"
                                    on_press={() =>
                                        run(repo.id, async () => {
                                            await create_worktree(repo.id, (names[repo.id] ?? "").trim());
                                            set_names((current) => ({ ...current, [repo.id]: "" }));
                                        })
                                    }
                                >
                                    create worktree
                                </Press>
                            </div>
                        </div>
                    </section>
                ))}
            </div>

            {preview ? (
                <aside className="flex w-[46%] min-w-[420px] flex-col border-l border-reef">
                    <div className="flex items-center justify-between border-b border-reef px-2 py-1 font-mono text-[11px] text-shell">
                        <span>{preview}</span>
                        <button
                            className="border border-foam px-2 py-1 rounded-lg"
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
