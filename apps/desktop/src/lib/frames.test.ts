import { describe, expect, it } from "vitest";

import { median } from "@/lib/frames";

describe("what a frame costs on this machine", () => {
    it("takes the middle of what was measured, not the worst", () => {
        expect(median([40, 41, 39, 520, 38])).toBe(40);
    });

    it("has no answer before anything has been drawn", () => {
        expect(median([])).toBe(0);
    });

    it("averages the two middle readings on an even sample", () => {
        expect(median([10, 20, 30, 40])).toBe(25);
    });
});
