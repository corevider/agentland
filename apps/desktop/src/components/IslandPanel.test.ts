import { describe, expect, it } from "vitest";

import { keep_inside, on_the_canvas } from "@/components/IslandPanel";

const canvas = { width: 600, height: 400 };

describe("where an agent's name tag may be drawn", () => {
    it("shows it over the scene", () => {
        expect(on_the_canvas({ x: 300, y: 200, visible: true }, canvas.width, canvas.height)).toBe(true);
    });

    it("keeps it off the cards beside the scene", () => {
        expect(on_the_canvas({ x: -12, y: 200, visible: true }, canvas.width, canvas.height)).toBe(false);
        expect(on_the_canvas({ x: 640, y: 200, visible: true }, canvas.width, canvas.height)).toBe(false);
        expect(on_the_canvas({ x: 300, y: -4, visible: true }, canvas.width, canvas.height)).toBe(false);
        expect(on_the_canvas({ x: 300, y: 460, visible: true }, canvas.width, canvas.height)).toBe(false);
    });

    it("keeps it hidden when the station is behind the camera", () => {
        expect(on_the_canvas({ x: 300, y: 200, visible: false }, canvas.width, canvas.height)).toBe(false);
    });
});

describe("keeping a label inside the scene", () => {
    it("leaves a label alone in the middle", () => {
        expect(keep_inside(300, 120, 600)).toBe(300);
    });

    it("slides one back from the left edge instead of cutting it in half", () => {
        expect(keep_inside(10, 120, 600)).toBe(62);
    });

    it("slides one back from the right edge", () => {
        expect(keep_inside(590, 120, 600)).toBe(538);
    });

    it("centres a label too wide for the scene rather than pushing it off", () => {
        expect(keep_inside(10, 800, 600)).toBe(300);
    });
});
