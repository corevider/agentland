import { Suspense, createContext, lazy, useContext, type ReactNode } from "react";

import { BoardPanel } from "@/components/BoardPanel";
import { MailPanel } from "@/components/MailPanel";
import { MemoryPanel } from "@/components/MemoryPanel";
import { RoutinesPanel } from "@/components/RoutinesPanel";
import { CrewPanel } from "@/components/CrewPanel";
import { DispatchPanel } from "@/components/DispatchPanel";
import { PreviewPanel } from "@/components/PreviewPanel";
import { RepoPanel } from "@/components/RepoPanel";
import { SkillsPanel } from "@/components/SkillsPanel";
import { TerminalsPanel } from "@/components/TerminalsPanel";
import type { PaneMetrics } from "@/components/TerminalPane";
import type { SessionInfo, Agent } from "@/lib/core";

const IslandPanel = lazy(() =>
    import("@/components/IslandPanel").then((module) => ({ default: module.IslandPanel })),
);

export interface PanelProps {
    active: boolean;
    instance: string;
}

export interface PanelEntry {
    id: string;
    label: string;
    hint: string;
    Component: (props: PanelProps) => ReactNode;
}

export interface WorkspaceServices {
    sessions: SessionInfo[];
    crew: Agent[];
    repositories: string[] | null;
    open_session: (id: string) => void;
    close_session: (id: string) => void;
    open_shell_in: (cwd: string) => void;
    focus_pane: (id: string) => void;
    focused_id: string | null;
    on_metrics: (id: string, value: PaneMetrics) => void;
}

const Services = createContext<WorkspaceServices | null>(null);

export function ServiceProvider({
    services,
    children,
}: {
    services: WorkspaceServices;
    children: ReactNode;
}) {
    return <Services.Provider value={services}>{children}</Services.Provider>;
}

export function use_services(): WorkspaceServices {
    const held = useContext(Services);
    if (!held) {
        throw new Error("a panel was rendered outside the workspace");
    }
    return held;
}

export const PANELS: PanelEntry[] = [
    {
        id: "island",
        label: "Island",
        hint: "the crew at a glance",
        Component: ({ active }) => (
            <Suspense
                fallback={
                    <div className="flex min-h-0 flex-1 items-center justify-center font-mono text-[11px] text-shell">
                        loading the island…
                    </div>
                }
            >
                <IslandPanel active={active} on_open_session={use_services().open_session} />
            </Suspense>
        ),
    },
    {
        id: "panes",
        label: "Terminals",
        hint: "what the agents are doing",
        Component: ({ active }) => <TerminalsPanel active={active} />,
    },
    {
        id: "board",
        label: "Board",
        hint: "cards and their evidence",
        Component: ({ active }) => <BoardPanel active={active} repositories={use_services().repositories} />,
    },
    {
        id: "preview",
        label: "Preview",
        hint: "a worktree's localhost",
        Component: ({ active }) => <PreviewPanel active={active} />,
    },
    {
        id: "repos",
        label: "Repositories",
        hint: "worktrees, ports, servers",
        Component: ({ active }) => <RepoPanel active={active} />,
    },
    {
        id: "crew",
        label: "Crew",
        hint: "hire, start, stop",
        Component: ({ active }) => <CrewPanel active={active} on_open_session={use_services().open_session} />,
    },
    {
        id: "mail",
        label: "Mail",
        hint: "what the crew tells each other",
        Component: ({ active }) => <MailPanel active={active} />,
    },
    {
        id: "dispatch",
        label: "Dispatch",
        hint: "what X decided, and why",
        Component: ({ active }) => <DispatchPanel active={active} />,
    },
    {
        id: "memory",
        label: "Memory",
        hint: "what the crew has learned, once you approve it",
        Component: ({ active }) => <MemoryPanel active={active} />,
    },
    {
        id: "routines",
        label: "Routines",
        hint: "the same brief, on a timer",
        Component: ({ active }) => <RoutinesPanel active={active} />,
    },
    {
        id: "skills",
        label: "Skills",
        hint: "what the crew knows how to do",
        Component: ({ active }) => <SkillsPanel active={active} />,
    },
];

export function panel_entry(id: string): PanelEntry | null {
    return PANELS.find((entry) => entry.id === id) ?? null;
}

export function is_known_panel(id: string): boolean {
    return PANELS.some((entry) => entry.id === id);
}
