import { useCallback, useEffect, useMemo, useState } from "react";

import {
    activate_workspace,
    list_agents,
    list_repos,
    list_workspaces,
    type Agent,
    type Repository,
    type Workspace,
} from "@/lib/core";
import { PRESENCE_COLOR } from "@/island/geometry";
import type { PanelId } from "@/workspace/layout";
import { PANELS } from "@/workspace/registry";

interface Props {
    visible: PanelId[];
    repositories: string[] | null;
    active_workspace: string | null;
    /// Called after a workspace has been made active, so the rest of the window
    /// follows the rail rather than the other way round.
    on_switched: () => void;
    on_open_repo: (repository: Repository) => void;
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
    active_workspace,
    on_switched,
    on_open_repo,
    counts,
    collapsed,
    on_collapse,
    on_open_panel,
    on_open_agent,
    footer,
}: Props) {
    const [repos, set_repos] = useState<Repository[]>([]);
    const [agents, set_agents] = useState<Agent[]>([]);
    const [workspaces, set_workspaces] = useState<Workspace[]>([]);
    const [folded, set_folded] = useState<Record<string, boolean>>({});
    const [switching, set_switching] = useState(false);

    const refresh = useCallback(async () => {
        const [listed, crew, held] = await Promise.all([list_repos(), list_agents(), list_workspaces()]);
        set_repos(listed);
        set_agents(crew);
        set_workspaces(held.workspaces);
    }, []);

    const switch_to = useCallback(
        (id: string) => {
            set_switching(false);
            activate_workspace(id)
                .then(() => {
                    on_switched();
                    return refresh();
                })
                .catch(() => undefined);
        },
        [on_switched, refresh],
    );

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
                    Workspace
                </h2>

                <button
                    className="flex w-full items-center gap-1.5 rounded border border-reef px-2 py-1 text-left text-[13px] text-linen hover:border-turquoise"
                    title="switch to another workspace"
                    onClick={() => set_switching(!switching)}
                >
                    <span className="truncate">
                        {workspaces.find((held) => held.id === active_workspace)?.name ?? "no workspace yet"}
                    </span>
                    <span className="ml-auto shrink-0 font-mono text-[9px] text-shade">
                        {switching ? "▴" : "▾"}
                    </span>
                </button>

                {switching ? (
                    <div className="mb-1 mt-0.5 rounded border border-foam bg-lagoon-deep py-0.5">
                        {workspaces.map((workspace) => {
                            const busy = agents.filter(
                                (agent) =>
                                    workspace.repository_ids.includes(agent.repository_id) &&
                                    agent.session_id !== null,
                            ).length;

                            return (
                                <button
                                    key={workspace.id}
                                    className={`flex w-full items-center gap-2 px-2 py-[3px] text-left text-[12px] hover:bg-shallow ${
                                        workspace.id === active_workspace ? "text-turquoise" : "text-shell"
                                    }`}
                                    onClick={() => switch_to(workspace.id)}
                                >
                                    <span className="truncate">{workspace.name}</span>
                                    <span className="ml-auto shrink-0 font-mono text-[9px] text-shade">
                                        {workspace.repository_ids.length}
                                        {busy > 0 ? ` · ${busy} live` : ""}
                                    </span>
                                </button>
                            );
                        })}
                    </div>
                ) : null}

                <h2 className="px-1 pb-0.5 pt-3 font-mono text-[9px] uppercase tracking-[0.16em] text-shade">
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
                    Projects
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
                            <div className="flex w-full items-center gap-1.5 rounded px-2 py-[3px] text-[13px] text-linen hover:bg-lagoon-deep/60">
                                <button
                                    className="w-2 shrink-0 font-mono text-[9px] text-shade hover:text-linen"
                                    title={shut ? "show the crew here" : "fold this project"}
                                    onClick={() => set_folded((held) => ({ ...held, [repo.id]: !shut }))}
                                >
                                    {shut ? "▸" : "▾"}
                                </button>
                                <button
                                    className="min-w-0 flex-1 truncate text-left hover:text-turquoise"
                                    title={repo.primary_path}
                                    onClick={() => on_open_repo(repo)}
                                >
                                    {repo.name}
                                </button>
                                <span className="font-mono text-[10px] tabular-nums text-shade">
                                    {crew.length}
                                </span>
                            </div>

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
