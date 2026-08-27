import { useCallback, useEffect, useMemo, useState } from "react";

import { list_agents, list_repos, type Agent, type Repository } from "@/lib/core";
import { PRESENCE_COLOR } from "@/island/geometry";
import type { PanelId } from "@/workspace/layout";
import { PANELS } from "@/workspace/registry";

interface Props {
    visible: PanelId[];
    repositories: string[] | null;
    counts: Partial<Record<PanelId, number>>;
    collapsed: boolean;
    on_collapse: (next: boolean) => void;
    on_open_panel: (panel: PanelId) => void;
    on_open_agent: (agent: Agent) => void;
    footer: React.ReactNode;
}

function Dot({ presence }: { presence: string }) {
    return (
        <span
            className="inline-block size-[7px] shrink-0 rounded-full"
            style={{ background: PRESENCE_COLOR[presence] ?? PRESENCE_COLOR.idle }}
        />
    );
}

export function WorkspaceRail({
    visible,
    repositories,
    counts,
    collapsed,
    on_collapse,
    on_open_panel,
    on_open_agent,
    footer,
}: Props) {
    const [repos, set_repos] = useState<Repository[]>([]);
    const [agents, set_agents] = useState<Agent[]>([]);
    const [folded, set_folded] = useState<Record<string, boolean>>({});

    const refresh = useCallback(async () => {
        const [listed, crew] = await Promise.all([list_repos(), list_agents()]);
        set_repos(listed);
        set_agents(crew);
    }, []);

    const shown_repos = useMemo(
        () => (repositories ? repos.filter((repo) => repositories.includes(repo.id)) : repos),
        [repos, repositories],
    );

    useEffect(() => {
        refresh().catch(() => undefined);
        const handle = window.setInterval(() => refresh().catch(() => undefined), 4000);
        return () => window.clearInterval(handle);
    }, [refresh]);

    const by_repo = useMemo(() => {
        const grouped = new Map<string, Agent[]>();
        for (const agent of agents) {
            const held = grouped.get(agent.repository_id) ?? [];
            held.push(agent);
            grouped.set(agent.repository_id, held);
        }
        return grouped;
    }, [agents]);

    if (collapsed) {
        return (
            <nav className="flex w-11 shrink-0 flex-col items-center gap-1 border-r border-reef/70 py-2">
                <button
                    className="mb-1 rounded px-2 py-1 font-mono text-[11px] text-shell hover:text-linen"
                    title="show the rail"
                    onClick={() => on_collapse(false)}
                >
                    »
                </button>
                {PANELS.map((panel) => (
                    <button
                        key={panel.id}
                        title={panel.label}
                        onClick={() => on_open_panel(panel.id)}
                        className={`w-7 rounded py-1 font-mono text-[11px] ${
                            visible.includes(panel.id) ? "bg-lagoon-deep text-turquoise" : "text-shade hover:text-linen"
                        }`}
                    >
                        {panel.label.slice(0, 2).toLowerCase()}
                    </button>
                ))}
            </nav>
        );
    }

    return (
        <nav className="flex w-[212px] shrink-0 flex-col border-r border-reef/70">
            <div className="flex items-center justify-between px-2 py-1">
                <span className="font-display text-[15px] font-semibold tracking-tight text-linen">
                    Agentland
                </span>
                <button
                    className="rounded px-1 font-mono text-[11px] text-shade hover:text-linen"
                    title="hide the rail"
                    onClick={() => on_collapse(true)}
                >
                    «
                </button>
            </div>

            <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
                <h2 className="px-1 pb-0.5 pt-1.5 font-mono text-[9px] uppercase tracking-[0.16em] text-shade">
                    Views
                </h2>
                {PANELS.map((panel) => {
                    const shown = visible.includes(panel.id);
                    const count = counts[panel.id];

                    return (
                        <button
                            key={panel.id}
                            title={panel.hint}
                            onClick={() => on_open_panel(panel.id)}
                            className={`flex w-full items-center gap-2 rounded px-2 py-[3px] text-left text-[13px] ${
                                shown ? "bg-lagoon-deep text-linen" : "text-shell hover:bg-lagoon-deep/60"
                            }`}
                        >
                            <span
                                className={`h-3 w-[2px] shrink-0 rounded ${shown ? "bg-turquoise" : "bg-transparent"}`}
                            />
                            <span className="truncate">{panel.label}</span>
                            {count ? (
                                <span className="ml-auto font-mono text-[10px] tabular-nums text-shade">{count}</span>
                            ) : null}
                        </button>
                    );
                })}

                <h2 className="px-1 pb-0.5 pt-3 font-mono text-[9px] uppercase tracking-[0.16em] text-shade">
                    Workspaces
                </h2>
                {shown_repos.length === 0 ? (
                    <p className="px-2 font-mono text-[10px] text-shade">
                        {repositories ? "This workspace holds no repository yet." : "No repository yet."}
                    </p>
                ) : null}

                {shown_repos.map((repo) => {
                    const crew = by_repo.get(repo.id) ?? [];
                    const shut = folded[repo.id] ?? false;

                    return (
                        <div key={repo.id} className="mb-0.5">
                            <button
                                onClick={() => set_folded((held) => ({ ...held, [repo.id]: !shut }))}
                                className="flex w-full items-center gap-1.5 rounded px-2 py-[3px] text-left text-[13px] text-linen hover:bg-lagoon-deep/60"
                            >
                                <span className="w-2 shrink-0 font-mono text-[9px] text-shade">
                                    {shut ? "▸" : "▾"}
                                </span>
                                <span className="truncate">{repo.name}</span>
                                <span className="ml-auto font-mono text-[10px] tabular-nums text-shade">
                                    {crew.length}
                                </span>
                            </button>

                            {shut
                                ? null
                                : crew.map((agent) => (
                                      <button
                                          key={agent.id}
                                          onClick={() => on_open_agent(agent)}
                                          title={agent.reason}
                                          className="flex w-full items-center gap-2 rounded py-[2px] pl-[24px] pr-2 text-left text-[12px] text-shell hover:bg-lagoon-deep/60 hover:text-linen"
                                      >
                                          <Dot presence={agent.presence} />
                                          <span className="truncate">{agent.name}</span>
                                          <span className="ml-auto truncate font-mono text-[10px] text-shade">
                                              {agent.worktree}
                                          </span>
                                      </button>
                                  ))}
                        </div>
                    );
                })}
            </div>

            <div className="shrink-0 border-t border-reef/70 px-2 py-1">{footer}</div>
        </nav>
    );
}
