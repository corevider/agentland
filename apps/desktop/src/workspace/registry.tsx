import type { MenuItem } from "@/components/ContextMenu";
import { Suspense, createContext, lazy, useContext, type ReactNode } from "react";

import { ActivityPanel } from "@/components/ActivityPanel";
import { ApprovalsPanel } from "@/components/ApprovalsPanel";
import { BoardPanel } from "@/components/BoardPanel";
import { MailPanel } from "@/components/MailPanel";
import { MemoryPanel } from "@/components/MemoryPanel";
import { NotesPanel } from "@/components/NotesPanel";
import { RoutinesPanel } from "@/components/RoutinesPanel";
import { CommanderPanel } from "@/components/CommanderPanel";
import { CrewPanel } from "@/components/CrewPanel";
import { DispatchPanel } from "@/components/DispatchPanel";
import { PreviewPanel } from "@/components/PreviewPanel";
import { ProjectPanel } from "@/components/ProjectPanel";
import { RepoPanel } from "@/components/RepoPanel";
import { SkillsPanel } from "@/components/SkillsPanel";
import { Waiting } from "@/components/Spinner";
import { StartPanel } from "@/components/StartPanel";
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
    /// Whether the panel must stay mounted while its tab is in the background.
    /// Only what holds live state needs this — a terminal owns a pty and an
    /// xterm buffer, and remounting it would lose both. Everything else is
    /// cheaper to rebuild than to keep drawing.
    keep_mounted?: boolean;
    Component: (props: PanelProps) => ReactNode;
}

export interface WorkspaceServices {
    open_menu: (event: React.MouseEvent, title: string | undefined, items: MenuItem[]) => void;
    sessions: SessionInfo[];
    crew: Agent[];
    repositories: string[] | null;
    /// Where the person last said they wanted to be, from the jumper: a project
    /// and, when they picked one, the worktree inside it.
    going: { repository_id: string | null; worktree: string | null; at: number } | null;
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
        id: "start",
        label: "Start",
        hint: "open a project, put a crew in it, say what to do",
        Component: ({ active }) => <StartPanel active={active} />,
    },
    {
        id: "island",
        label: "Island",
        hint: "the crew at a glance",
        keep_mounted: true,
        Component: ({ active }) => (
            <Suspense
                fallback={
                    <div className="flex min-h-0 flex-1 items-center justify-center font-mono text-[11px] text-shell">
                        <Waiting says="loading the island…" />
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
        keep_mounted: true,
        Component: ({ active }) => <TerminalsPanel active={active} />,
    },
    {
        id: "commander",
        label: "Commander",
        hint: "X's plans and what the supervisor is following",
        Component: ({ active }) => <CommanderPanel active={active} />,
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
        id: "project",
        label: "Files & Git",
        hint: "a project's folder, and what a branch changed in it",
        Component: ({ active }) => (
            <ProjectPanel
                active={active}
                repositories={use_services().repositories}
                going={use_services().going}
            />
        ),
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
        id: "approvals",
        label: "Approvals",
        hint: "the agents that are blocked on you",
        Component: ({ active }) => <ApprovalsPanel active={active} />,
    },
    {
        id: "dispatch",
        label: "Dispatch",
        hint: "what X decided, and why",
        Component: ({ active }) => <DispatchPanel active={active} />,
    },
    {
        id: "activity",
        label: "Activity",
        hint: "what it spent, and what it did",
        Component: ({ active }) => <ActivityPanel active={active} />,
    },
    {
        id: "memory",
        label: "Memory",
        hint: "what the crew has learned, once you approve it",
        Component: ({ active }) => <MemoryPanel active={active} />,
    },
    {
        id: "notes",
        label: "Notes",
        hint: "the vault the crew writes into, and you can open in any note tool",
        Component: ({ active }) => <NotesPanel active={active} />,
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
