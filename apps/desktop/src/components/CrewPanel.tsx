import { use_poll } from "@/lib/poll";
import { useCallback, useEffect, useState } from "react";

import {
    shape_agent,
    dismiss_agent,
    format_elapsed,
    hire_agent,
    list_agents,
    list_engines,
    list_repos,
    list_worktrees,
    session_stats,
    start_agent,
    stop_agent,
    type Agent,
    type Engine,
    type SessionInfo,
} from "@/lib/core";

const ROLES = ["implementer", "reviewer", "tester", "researcher", "ops", "commander"];

const PRESENCE_COLOR: Record<string, string> = {
    done: "text-palm",
    working: "text-sun",
    waiting: "text-turquoise",
    attention: "text-coral",
    idle: "text-shell",
};

const PRESENCE_LABEL: Record<string, string> = {
    done: "finished",
    working: "working",
    waiting: "at a prompt",
    attention: "needs you",
    idle: "idle",
};

interface Props {
    active: boolean;
    on_open_session: (session_id: string) => void;
}

export function CrewPanel({ active, on_open_session }: Props) {
    const [engines, set_engines] = useState<Engine[]>([]);
    const [agents, set_agents] = useState<Agent[]>([]);
    const [targets, set_targets] = useState<Array<{ repository_id: string; worktree: string }>>([]);
    const [draft, set_draft] = useState({ name: "", role: ROLES[0], engine_id: "", target: "" });
    const [error, set_error] = useState<string | null>(null);
    const [busy, set_busy] = useState(false);
    const [activity, set_activity] = useState<Record<string, SessionInfo>>({});
    const [now, set_now] = useState(() => Math.floor(Date.now() / 1000));

    const refresh = useCallback(async () => {
        const [available, crew, repos] = await Promise.all([
            list_engines(),
            list_agents(),
            list_repos(),
        ]);

        set_engines(available);
        set_agents(crew);

        const lists = await Promise.all(repos.map((repo) => list_worktrees(repo.id)));
        const flat = lists.flat().map((entry) => ({
            repository_id: entry.repository_id,
            worktree: entry.name,
        }));
        set_targets(flat);

        set_draft((current) => ({
            ...current,
            engine_id: current.engine_id || available.find((entry) => entry.installed)?.id || "",
            target: current.target || (flat[0] ? `${flat[0].repository_id}/${flat[0].worktree}` : ""),
        }));
    }, []);

    use_poll(() => {
        list_agents().then(set_agents).catch(() => undefined);
    }, 3000, active);

    useEffect(() => {
        if (!active) {
            return;
        }

        refresh().catch((cause) => set_error(String(cause)));

        const ticker = window.setInterval(() => set_now(Math.floor(Date.now() / 1000)), 1000);
        return () => window.clearInterval(ticker);
    }, [refresh, active]);

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

    useEffect(() => {
        const running = agents.filter((agent) => agent.session_id);
        if (running.length === 0) {
            return;
        }

        Promise.all(
            running.map((agent) =>
                session_stats(agent.session_id as string)
                    .then((value) => [agent.id, value] as const)
                    .catch(() => null),
            ),
        ).then((entries) => {
            set_activity(Object.fromEntries(entries.filter(Boolean) as Array<[string, SessionInfo]>));
        });
    }, [agents, now]);

    const installed = engines.filter((entry) => entry.installed);

    return (
        <div className="flex h-full min-h-0 min-w-0 flex-1 flex-col gap-5 overflow-y-auto p-2.5">
            <section>
                <h2 className="mb-1 font-mono text-[11px] uppercase tracking-[0.12em] text-shell">
                    Engines on this machine
                </h2>
                <div className="flex flex-wrap gap-2">
                    {engines.map((engine) => (
                        <span
                            key={engine.id}
                            className={`border px-2 py-1 font-mono text-[11px] ${
                                engine.installed
                                    ? "border-turquoise text-turquoise"
                                    : "border-reef text-shade"
                            }`}
                            title={engine.version ?? "not installed"}
                        >
                            {engine.id}
                        </span>
                    ))}
                </div>
            </section>

            <section className="border border-reef bg-lagoon p-2 rounded-lg">
                <h2 className="mb-2 font-mono text-[11px] uppercase tracking-[0.12em] text-shell">
                    Hire
                </h2>

                {installed.length === 0 ? (
                    <p className="font-mono text-[11px] text-sun">
                        No agent CLI found on PATH. Install one — Claude Code, Codex, Gemini — and it
                        appears here.
                    </p>
                ) : (
                    <div className="flex flex-wrap items-center gap-2">
                        <input
                            className="w-40 border border-reef bg-lagoon-deep px-2 py-1 font-mono text-[11px] rounded-lg"
                            placeholder="name"
                            value={draft.name}
                            onChange={(event) => set_draft({ ...draft, name: event.target.value })}
                        />
                        <select
                            className="border border-reef bg-lagoon-deep px-2 py-1 font-mono text-[11px] rounded-lg"
                            value={draft.role}
                            onChange={(event) => set_draft({ ...draft, role: event.target.value })}
                        >
                            {ROLES.map((role) => (
                                <option key={role} value={role}>
                                    {role}
                                </option>
                            ))}
                        </select>
                        <select
                            className="border border-reef bg-lagoon-deep px-2 py-1 font-mono text-[11px] rounded-lg"
                            value={draft.engine_id}
                            onChange={(event) => set_draft({ ...draft, engine_id: event.target.value })}
                        >
                            {installed.map((engine) => (
                                <option key={engine.id} value={engine.id}>
                                    {engine.name}
                                </option>
                            ))}
                        </select>
                        <select
                            className="border border-reef bg-lagoon-deep px-2 py-1 font-mono text-[11px] rounded-lg"
                            value={draft.target}
                            onChange={(event) => set_draft({ ...draft, target: event.target.value })}
                        >
                            {targets.map((target) => {
                                const value = `${target.repository_id}/${target.worktree}`;
                                return (
                                    <option key={value} value={value}>
                                        {value}
                                    </option>
                                );
                            })}
                        </select>
                        <button
                            className="border border-turquoise px-2 py-0.5 font-mono text-[11px] text-turquoise disabled:opacity-40 rounded-lg"
                            disabled={busy || !draft.name.trim() || !draft.target}
                            onClick={() =>
                                run(async () => {
                                    const [repository_id, worktree] = draft.target.split("/");
                                    await hire_agent({
                                        name: draft.name.trim(),
                                        role: draft.role,
                                        engine_id: draft.engine_id,
                                        repository_id,
                                        worktree,
                                    });
                                    set_draft({ ...draft, name: "" });
                                })
                            }
                        >
                            hire
                        </button>
                    </div>
                )}
            </section>

            {error ? (
                <div className="border border-coral bg-lagoon px-2 py-1 font-mono text-[11px] text-coral rounded-lg">
                    {error}
                </div>
            ) : null}

            <section className="flex flex-col gap-2">
                {agents.length === 0 ? (
                    <p className="font-mono text-[11px] text-shell">
                        No crew yet. An agent is a name, a role, an engine and a worktree it owns.
                    </p>
                ) : null}

                {agents.map((agent) => (
                    <div
                        key={agent.id}
                        className="flex flex-wrap items-center gap-3 border border-reef bg-lagoon px-2 py-1 font-mono text-[11px] rounded-lg"
                    >
                        <span className="flex items-center gap-1.5 text-linen">
                            {agent.colour ? (
                                <span
                                    className="inline-block size-[9px] rounded-full"
                                    style={{ backgroundColor: agent.colour }}
                                    title={`known by ${agent.colour}`}
                                />
                            ) : null}
                            {agent.name}
                        </span>
                        <span className="text-shell">{agent.role}</span>
                        <span className="text-turquoise">
                            {agent.engine_id}
                            {agent.model ? ` · ${agent.model}` : ""}
                        </span>
                        <select
                            className={`rounded border border-reef bg-lagoon-deep px-1 py-[1px] font-mono text-[10px] ${
                                agent.permissions === "bypassPermissions" ? "text-coral" : "text-shade"
                            }`}
                            title="how much this agent does without asking — yours to set"
                            value={agent.permissions ?? ""}
                            onChange={(event) => {
                                shape_agent(agent.id, { permissions: event.target.value })
                                    .then(() => refresh())
                                    .catch((cause) => set_error(String(cause)));
                            }}
                        >
                            <option value="">role default</option>
                            <option value="plan">plan · reads</option>
                            <option value="default">default · asks first</option>
                            <option value="acceptEdits">acceptEdits · writes, asks to run</option>
                            <option value="bypassPermissions">bypassPermissions · never asks</option>
                        </select>
                        <span className="text-shell">
                            {agent.repository_id}/{agent.worktree}
                        </span>
                        <span
                            className={PRESENCE_COLOR[agent.presence] ?? PRESENCE_COLOR.idle}
                            title={agent.reason}
                        >
                            {PRESENCE_LABEL[agent.presence] ?? agent.presence}
                            {activity[agent.id]
                                ? ` ${format_elapsed(now - activity[agent.id].last_output_at)}`
                                : ""}
                        </span>

                        <div className="ml-auto flex items-center gap-2">
                            {agent.session_id ? (
                                <>
                                    <button
                                        className="border border-foam rounded-md px-1.5 py-0.5 text-[11px]"
                                        onClick={() => on_open_session(agent.session_id as string)}
                                    >
                                        open pane
                                    </button>
                                    <button
                                        className="border border-foam px-2 py-1 text-[11px] disabled:opacity-40 rounded-lg"
                                        disabled={busy}
                                        onClick={() => run(() => stop_agent(agent.id))}
                                    >
                                        stop
                                    </button>
                                </>
                            ) : (
                                <>
                                    <button
                                        className="border border-foam px-2 py-1 text-[11px] disabled:opacity-40 rounded-lg"
                                        disabled={busy}
                                        onClick={() => run(() => start_agent(agent.id, false))}
                                    >
                                        start
                                    </button>
                                    <button
                                        className="border border-foam px-2 py-1 text-[11px] disabled:opacity-40 rounded-lg"
                                        disabled={busy}
                                        onClick={() => run(() => start_agent(agent.id, true))}
                                    >
                                        resume
                                    </button>
                                </>
                            )}
                            <button
                                className="border border-coral rounded-md px-1.5 py-0.5 text-[11px] text-coral disabled:opacity-40"
                                disabled={busy}
                                onClick={() => run(() => dismiss_agent(agent.id))}
                            >
                                dismiss
                            </button>
                        </div>
                    </div>
                ))}
            </section>
        </div>
    );
}
