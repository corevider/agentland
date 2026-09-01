import { describe, expect, it } from "vitest";

import { SPINNER_FRAMES, spin_frame } from "@/lib/spin";

describe("the frame a spinner is on", () => {
    it("walks the cycle and comes back round", () => {
        expect(spin_frame(0)).toBe(SPINNER_FRAMES[0]);
        expect(spin_frame(3)).toBe(SPINNER_FRAMES[3]);
        expect(spin_frame(SPINNER_FRAMES.length)).toBe(SPINNER_FRAMES[0]);
        expect(spin_frame(SPINNER_FRAMES.length + 2)).toBe(SPINNER_FRAMES[2]);
    });

    it("is a frame at every tick, however the counter arrived", () => {
        for (const tick of [-1, -13, 0, 1, 9999]) {
            expect(SPINNER_FRAMES, `tick ${tick}`).toContain(spin_frame(tick));
        }
    });

    it("is one glyph wide, so a line does not shift as it turns", () => {
        for (const frame of SPINNER_FRAMES) {
            expect([...frame]).toHaveLength(1);
        }
    });
});
