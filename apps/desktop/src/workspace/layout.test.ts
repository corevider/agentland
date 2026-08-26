import { describe, expect, it } from "vitest";

import {
    DEFAULT_LAYOUT,
    close_panel,
    move_panel,
    open_panel,
    upgrade_layout,
    visible_panels,
    type Layout,
} from "@/workspace/layout";

function layout_with(panels: Partial<Record<keyof Layout["slots"], string[]>>): Layout {
    return {
        ...DEFAULT_LAYOUT,
        slots: {
            left_top: { panels: (panels.left_top ?? []) as never, active: 0 },
            left_bottom: { panels: (panels.left_bottom ?? []) as never, active: 0 },
            right_top: { panels: (panels.right_top ?? []) as never, active: 0 },
            right_bottom: { panels: (panels.right_bottom ?? []) as never, active: 0 },
        },
    };
}

describe("a layout saved by the previous version", () => {
    it("becomes the four-slot layout without losing a panel", () => {
        const upgraded = upgrade_layout({
            left: "island",
            right: "panes",
            bottom: "board",
            left_fraction: 0.34,
            bottom_fraction: 0.3,
        });

        expect(upgraded.slots.left_top.panels).toEqual(["island"]);
        expect(upgraded.slots.right_top.panels).toEqual(["panes"]);
        expect(upgraded.slots.left_bottom.panels).toEqual(["board"]);
        expect(upgraded.slots.right_bottom.panels).toEqual([]);
        expect(upgraded.column_fraction).toBe(0.34);
        expect(upgraded.left_row_fraction).toBeCloseTo(0.7);
    });

    it("drops a panel that no longer exists rather than rendering nothing", () => {
        const upgraded = upgrade_layout({
            slots: {
                left_top: { panels: ["island", "seance"], active: 1 },
                left_bottom: { panels: [], active: 0 },
                right_top: { panels: ["panes"], active: 0 },
                right_bottom: { panels: [], active: 0 },
            },
            column_fraction: 0.4,
        });

        expect(upgraded.slots.left_top.panels).toEqual(["island"]);
        expect(upgraded.slots.left_top.active).toBe(0);
    });
});

describe("opening a panel", () => {
    it("selects the tab it is already in instead of duplicating it", () => {
        const layout = layout_with({ left_top: ["island", "board"], right_top: ["panes"] });
        const next = open_panel(layout, "board");

        expect(next.slots.left_top.panels).toEqual(["island", "board"]);
        expect(next.slots.left_top.active).toBe(1);
    });

    it("fills an empty slot before crowding a full one", () => {
        const layout = layout_with({ left_top: ["island"], right_top: ["panes"] });
        const next = open_panel(layout, "preview");

        expect(next.slots.left_bottom.panels).toEqual(["preview"]);
    });
});

describe("moving a panel between slots", () => {
    it("leaves no copy behind", () => {
        const layout = layout_with({ left_top: ["island", "board"], right_top: ["panes"] });
        const next = move_panel(layout, "board", "right_bottom");

        expect(next.slots.left_top.panels).toEqual(["island"]);
        expect(next.slots.right_bottom.panels).toEqual(["board"]);
        expect(visible_panels(next)).toContain("board");
    });

    it("keeps the source slot pointing at a tab that still exists", () => {
        const layout = {
            ...layout_with({ left_top: ["island", "board", "crew"] }),
        };
        layout.slots.left_top.active = 2;

        const next = move_panel(layout, "crew", "right_top");
        expect(next.slots.left_top.active).toBe(1);
        expect(next.slots.left_top.panels[next.slots.left_top.active]).toBe("board");
    });
});

describe("closing a tab", () => {
    it("falls back to a remaining tab", () => {
        const layout = layout_with({ left_top: ["island", "board"] });
        layout.slots.left_top.active = 1;

        const next = close_panel(layout, "left_top", "board");
        expect(next.slots.left_top.panels).toEqual(["island"]);
        expect(next.slots.left_top.active).toBe(0);
    });

    it("leaves an empty slot that can still receive a drop", () => {
        const layout = layout_with({ left_bottom: ["board"] });
        const next = close_panel(layout, "left_bottom", "board");

        expect(next.slots.left_bottom.panels).toEqual([]);
        expect(visible_panels(next)).not.toContain("board");
    });
});
