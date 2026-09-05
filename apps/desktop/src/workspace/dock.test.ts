import { describe, expect, it } from "vitest";

import { zone_at, zone_rect, zone_says } from "@/workspace/dock";

const box = { left: 100, top: 100, width: 400, height: 400 };

describe("where a dragged tab lands on a stack", () => {
    it("takes the middle as a tab", () => {
        expect(zone_at(300, 300, box)).toBe("center");
    });

    it("splits at the edges, each edge its own side", () => {
        expect(zone_at(110, 300, box)).toBe("left");
        expect(zone_at(490, 300, box)).toBe("right");
        expect(zone_at(300, 110, box)).toBe("top");
        expect(zone_at(300, 490, box)).toBe("bottom");
    });

    it("in a corner, goes to the edge the pointer is nearer", () => {
        expect(zone_at(105, 150, box)).toBe("left");
        expect(zone_at(150, 105, box)).toBe("top");
    });

    it("keeps a middle on a small stack and a reachable edge on a big one", () => {
        const small = { left: 0, top: 0, width: 100, height: 100 };
        expect(zone_at(50, 50, small)).toBe("center");
        const big = { left: 0, top: 0, width: 2000, height: 2000 };
        expect(zone_at(120, 1000, big)).toBe("left");
        expect(zone_at(400, 1000, big)).toBe("center");
    });

    it("draws the half it would take, or the inset middle", () => {
        expect(zone_rect("right")).toEqual({ left: 0.5, top: 0, width: 0.5, height: 1 });
        expect(zone_rect("bottom")).toEqual({ left: 0, top: 0.5, width: 1, height: 0.5 });
        expect(zone_rect("center").width).toBeLessThan(1);
    });

    it("says what would happen", () => {
        expect(zone_says("center", false)).toBe("add as a tab");
        expect(zone_says("center", true)).toBe("already here");
        expect(zone_says("left", true)).toBe("split beside");
        expect(zone_says("bottom", false)).toBe("split below");
    });
});
