import { describe, expect, it } from "vitest";

import { PANELS, is_known_panel, panel_entry } from "@/workspace/registry";

describe("the panel registry", () => {
    it("gives every panel a distinct id", () => {
        const ids = PANELS.map((panel) => panel.id);
        expect(new Set(ids).size).toBe(ids.length);
    });

    it("gives every panel something to show in a tab and a menu", () => {
        for (const panel of PANELS) {
            expect(panel.label.trim().length).toBeGreaterThan(0);
            expect(panel.hint.trim().length).toBeGreaterThan(0);
            expect(typeof panel.Component).toBe("function");
        }
    });

    it("is what decides whether a stored layout may restore a panel", () => {
        for (const panel of PANELS) {
            expect(is_known_panel(panel.id)).toBe(true);
            expect(panel_entry(panel.id)?.label).toBe(panel.label);
        }

        expect(is_known_panel("seance")).toBe(false);
        expect(panel_entry("seance")).toBeNull();
    });
});
