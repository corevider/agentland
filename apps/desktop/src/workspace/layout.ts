export type PanelId = "island" | "panes" | "board" | "repos" | "crew" | "skills" | "preview";

export interface PanelMeta {
    id: PanelId;
    label: string;
    hint: string;
}

export const PANELS: PanelMeta[] = [
    { id: "island", label: "Island", hint: "the crew at a glance" },
    { id: "panes", label: "Terminals", hint: "what the agents are doing" },
    { id: "board", label: "Board", hint: "cards and their evidence" },
    { id: "preview", label: "Preview", hint: "a worktree's localhost" },
    { id: "repos", label: "Repositories", hint: "worktrees, ports, servers" },
    { id: "crew", label: "Crew", hint: "hire, start, stop" },
    { id: "skills", label: "Skills", hint: "what the crew knows how to do" },
];

export const SLOTS = ["left_top", "left_bottom", "right_top", "right_bottom"] as const;
export type SlotId = (typeof SLOTS)[number];

export interface Slot {
    panels: PanelId[];
    active: number;
}

export interface Layout {
    slots: Record<SlotId, Slot>;
    maximised?: SlotId | null;
    column_fraction: number;
    left_row_fraction: number;
    right_row_fraction: number;
}

export const DEFAULT_LAYOUT: Layout = {
    slots: {
        left_top: { panels: ["island"], active: 0 },
        left_bottom: { panels: ["board"], active: 0 },
        right_top: { panels: ["panes"], active: 0 },
        right_bottom: { panels: [], active: 0 },
    },
    maximised: null,
    column_fraction: 0.38,
    left_row_fraction: 0.58,
    right_row_fraction: 0.62,
};

const STORAGE_KEY = "agentland-layout";

interface LegacyLayout {
    left?: PanelId | null;
    right?: PanelId | null;
    bottom?: PanelId | null;
    left_fraction?: number;
    bottom_fraction?: number;
}

function is_legacy(value: unknown): value is LegacyLayout {
    return typeof value === "object" && value !== null && !("slots" in value);
}

function slot_of(panel: PanelId | null | undefined): Slot {
    return panel ? { panels: [panel], active: 0 } : { panels: [], active: 0 };
}

export function upgrade_layout(stored: unknown): Layout {
    if (is_legacy(stored)) {
        return {
            slots: {
                left_top: slot_of(stored.left),
                left_bottom: slot_of(stored.bottom),
                right_top: slot_of(stored.right),
                right_bottom: { panels: [], active: 0 },
            },
            column_fraction: stored.left_fraction ?? DEFAULT_LAYOUT.column_fraction,
            left_row_fraction: stored.bottom_fraction
                ? 1 - stored.bottom_fraction
                : DEFAULT_LAYOUT.left_row_fraction,
            right_row_fraction: DEFAULT_LAYOUT.right_row_fraction,
        };
    }

    const layout = stored as Partial<Layout>;
    const slots = {} as Record<SlotId, Slot>;
    const known = new Set<string>(PANELS.map((panel) => panel.id));

    for (const id of SLOTS) {
        const slot = layout.slots?.[id];
        const panels = (slot?.panels ?? []).filter((panel) => known.has(panel));
        slots[id] = {
            panels,
            active: Math.min(Math.max(slot?.active ?? 0, 0), Math.max(panels.length - 1, 0)),
        };
    }

    return {
        slots,
        maximised: SLOTS.includes(layout.maximised as SlotId) ? (layout.maximised as SlotId) : null,
        column_fraction: layout.column_fraction ?? DEFAULT_LAYOUT.column_fraction,
        left_row_fraction: layout.left_row_fraction ?? DEFAULT_LAYOUT.left_row_fraction,
        right_row_fraction: layout.right_row_fraction ?? DEFAULT_LAYOUT.right_row_fraction,
    };
}

export function load_layout(): Layout {
    try {
        const raw = localStorage.getItem(STORAGE_KEY);
        if (!raw) {
            return DEFAULT_LAYOUT;
        }
        return upgrade_layout(JSON.parse(raw));
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
    return Math.min(0.82, Math.max(0.18, value));
}

export function visible_panels(layout: Layout): PanelId[] {
    return SLOTS.flatMap((id) => {
        const slot = layout.slots[id];
        const panel = slot.panels[slot.active];
        return panel ? [panel] : [];
    });
}

export function open_panel(layout: Layout, panel: PanelId): Layout {
    for (const id of SLOTS) {
        const index = layout.slots[id].panels.indexOf(panel);
        if (index >= 0) {
            return {
                ...layout,
                slots: { ...layout.slots, [id]: { ...layout.slots[id], active: index } },
            };
        }
    }

    const empty = SLOTS.find((id) => layout.slots[id].panels.length === 0);
    const target: SlotId = empty ?? "right_top";
    const slot = layout.slots[target];

    return {
        ...layout,
        slots: {
            ...layout.slots,
            [target]: { panels: [...slot.panels, panel], active: slot.panels.length },
        },
    };
}

export function move_panel(layout: Layout, panel: PanelId, into: SlotId): Layout {
    const slots = { ...layout.slots };

    for (const id of SLOTS) {
        const panels = slots[id].panels.filter((entry) => entry !== panel);
        if (panels.length !== slots[id].panels.length) {
            slots[id] = {
                panels,
                active: Math.min(slots[id].active, Math.max(panels.length - 1, 0)),
            };
        }
    }

    const target = slots[into];
    slots[into] = { panels: [...target.panels, panel], active: target.panels.length };

    return { ...layout, slots };
}

export function close_panel(layout: Layout, slot_id: SlotId, panel: PanelId): Layout {
    const slot = layout.slots[slot_id];
    const panels = slot.panels.filter((entry) => entry !== panel);

    return {
        ...layout,
        slots: {
            ...layout.slots,
            [slot_id]: { panels, active: Math.min(slot.active, Math.max(panels.length - 1, 0)) },
        },
    };
}
