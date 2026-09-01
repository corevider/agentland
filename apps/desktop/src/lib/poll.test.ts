import { describe, expect, it } from "vitest";

import { AWAY_FACTOR, next_delay } from "@/lib/poll";

describe("how often to ask the core again", () => {
    it("keeps the pace of the window being used", () => {
        expect(next_delay(3000, { hidden: false, focused: true })).toBe(3000);
    });

    it("slows down for a window sitting behind another", () => {
        expect(next_delay(3000, { hidden: false, focused: false })).toBe(3000 * AWAY_FACTOR);
    });

    it("stops for a window nobody can see", () => {
        expect(next_delay(3000, { hidden: true, focused: false })).toBe(0);
        expect(next_delay(3000, { hidden: true, focused: true })).toBe(0);
    });
});
