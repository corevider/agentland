import { useCallback, useEffect, useState } from "react";

import {
    dismiss_agent,
    hire_agent,
    list_agents,
    list_engines,
    list_repos,
    list_worktrees,
    start_agent,
    stop_agent,
    type Agent,
    type Engine,
} from "@/lib/core";

const ROLES = ["implementer", "reviewer", "tester", "researcher", "ops"];

const STATE_COLOR: Record<Agent["state"], string> = {
    working: "text-[#5aa87c]",
    idle: "text-[#c99a2e]",
    offline: "text-[#7b8d94]",
};

interface Props {
    on_open_session: (session_id: string) => void;
}

export function CrewPanel({ on_open_session }: Props) {
    const [engines, set_engines] = useState<Engine[]>([]);
    const [agents, set_agents] = useState<Agent[]>([]);
    const [targets, set_targets] = useState<Array<{ repository_id: string; worktree: string }>>([]);
    const [draft, set_draft] = useState({ name: "", role: ROLES[0], engine_id: "", target: "" });
    const [error, set_error] = useState<string | null>(null);
    const [busy, set_busy] = useState(false);

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

    useEffect(() => {
        refresh().catch((cause) => set_error(String(cause)));
        const handle = window.setInterval(() => {
            list_agents().then(set_agents).catch(() => undefined);
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

    const installed = engines.filter((entry) => entry.installed);

    return (
        <div className="flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto p-4">
            <section>
                <h2 className="mb-2 font-mono text-xs uppercase tracking-[0.12em] text-[#7b8d94]">
                    Engines on this machine
                </h2>
                <div className="flex flex-wrap gap-2">
                    {engines.map((engine) => (
                        <span
                            key={engine.id}
                            className={`border px-2 py-1 font-mono text-[11px] ${
                                engine.installed
                                    ? "border-[#45bcc4] text-[#45bcc4]"
                                    : "border-[#26343a] text-[#5d6e75]"
                            }`}
                            title={engine.version ?? "not installed"}
                        >
                            {engine.id}
                        </span>
                    ))}
                </div>
            </section>

            <section className="border border-[#26343a] bg-[#141c1f] p-3">
                <h2 className="mb-3 font-mono text-xs uppercase tracking-[0.12em] text-[#7b8d94]">
                    Hire
                </h2>

                {installed.length === 0 ? (
                    <p className="font-mono text-xs text-[#c99a2e]">
                        No agent CLI found on PATH. Install one — Claude Code, Codex, Gemini — and it
                        appears here.
                    </p>
                ) : (
                    <div className="flex flex-wrap items-center gap-2">
                        <input
                            className="w-40 border border-[#26343a] bg-[#0d1315] px-2 py-1 font-mono text-xs"
                            placeholder="name"
                            value={draft.name}
                            onChange={(event) => set_draft({ ...draft, name: event.target.value })}
                        />
                        <select
                            className="border border-[#26343a] bg-[#0d1315] px-2 py-1 font-mono text-xs"
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
                            className="border border-[#26343a] bg-[#0d1315] px-2 py-1 font-mono text-xs"
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
                            className="border border-[#26343a] bg-[#0d1315] px-2 py-1 font-mono text-xs"
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
                            className="border border-[#45bcc4] px-3 py-1 font-mono text-xs text-[#45bcc4] disabled:opacity-40"
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
                <div className="border border-[#d46969] bg-[#1b1113] px-3 py-2 font-mono text-xs text-[#d46969]">
                    {error}
                </div>
            ) : null}

            <section className="flex flex-col gap-2">
                {agents.length === 0 ? (
                    <p className="font-mono text-xs text-[#7b8d94]">
                        No crew yet. An agent is a name, a role, an engine and a worktree it owns.
                    </p>
                ) : null}

                {agents.map((agent) => (
                    <div
                        key={agent.id}
                        className="flex flex-wrap items-center gap-3 border border-[#26343a] bg-[#141c1f] px-3 py-2 font-mono text-xs"
                    >
                        <span className="text-[#e3ebee]">{agent.name}</span>
                        <span className="text-[#7b8d94]">{agent.role}</span>
                        <span className="text-[#45bcc4]">{agent.engine_id}</span>
                        <span className="text-[#7b8d94]">
                            {agent.repository_id}/{agent.worktree}
                        </span>
                        <span className={STATE_COLOR[agent.state]}>{agent.state}</span>

                        <div className="ml-auto flex items-center gap-2">
                            {agent.session_id ? (
                                <>
                                    <button
                                        className="border border-[#3a4d55] px-2 py-1 text-[11px]"
                                        onClick={() => on_open_session(agent.session_id as string)}
                                    >
                                        open pane
                                    </button>
                                    <button
                                        className="border border-[#3a4d55] px-2 py-1 text-[11px] disabled:opacity-40"
                                        disabled={busy}
                                        onClick={() => run(() => stop_agent(agent.id))}
                                    >
                                        stop
                                    </button>
                                </>
                            ) : (
                                <>
                                    <button
                                        className="border border-[#3a4d55] px-2 py-1 text-[11px] disabled:opacity-40"
                                        disabled={busy}
                                        onClick={() => run(() => start_agent(agent.id, false))}
                                    >
                                        start
                                    </button>
                                    <button
                                        className="border border-[#3a4d55] px-2 py-1 text-[11px] disabled:opacity-40"
                                        disabled={busy}
                                        onClick={() => run(() => start_agent(agent.id, true))}
                                    >
                                        resume
                                    </button>
                                </>
                            )}
                            <button
                                className="border border-[#9e3535] px-2 py-1 text-[11px] text-[#d46969] disabled:opacity-40"
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
