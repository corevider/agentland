import { describe, expect, it } from "vitest";

import { dated } from "@/lib/dated";

describe("dating a card", () => {
    it("uses the card's own date when it has one", () => {
        expect(dated(1_788_000_000, [{ at: 1_788_100_000 }])).toBe(1_788_000_000);
    });

    it("falls back to the latest thing that happened on it", () => {
        expect(dated(0, [{ at: 1_788_000_000 }, { at: 1_788_100_000 }, {}])).toBe(1_788_100_000);
    });

    it("stays undated when nothing on it was dated either", () => {
        expect(dated(undefined, [{}, { at: 0 }])).toBe(0);
        expect(dated(0, [])).toBe(0);
    });
});
