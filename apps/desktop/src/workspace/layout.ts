export type PanelId = "island" | "panes" | "board" | "repos" | "crew" | "skills";

export interface PanelMeta {
    id: PanelId;
    label: string;
    hint: string;
}

export const PANELS: PanelMeta[] = [
    { id: "island", label: "Island", hint: "the crew at a glance" },
    { id: "panes", label: "Terminals", hint: "what the agents are doing" },
    { id: "board", label: "Board", hint: "cards and their evidence" },
    { id: "repos", label: "Repositories", hint: "worktrees, ports, servers" },
    { id: "crew", label: "Crew", hint: "hire, start, stop" },
    { id: "skills", label: "Skills", hint: "what the crew knows how to do" },
];

export type SlotId = "left" | "right" | "bottom";

export interface Layout {
    left: PanelId | null;
    right: PanelId | null;
    bottom: PanelId | null;
    left_fraction: number;
    bottom_fraction: number;
}

export const DEFAULT_LAYOUT: Layout = {
    left: "island",
    right: "panes",
    bottom: "board",
    left_fraction: 0.34,
    bottom_fraction: 0.3,
};

const STORAGE_KEY = "agentland-layout";

export function load_layout(): Layout {
    try {
        const raw = localStorage.getItem(STORAGE_KEY);
        if (!raw) {
            return DEFAULT_LAYOUT;
        }
        return { ...DEFAULT_LAYOUT, ...(JSON.parse(raw) as Partial<Layout>) };
    } catch {
        return DEFAULT_LAYOUT;
    }
}

export function save_layout(layout: Layout): void {
    try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(layout));
    } catch {
        // a layout that cannot persist is still a usable layout
    }
}

export function clamp_fraction(value: number): number {
    return Math.min(0.75, Math.max(0.18, value));
}
