import { describe, expect, it } from "vitest";

import { clear_is_recommended } from "@/lib/commander";

const rested = { has_pane: true, running_plans: 0, open_cards: 0, finished_anything: true };

describe("recommending a clear chat to the commander", () => {
    it("recommends it once everything it held is over", () => {
        expect(clear_is_recommended(rested)).toBe(true);
    });

    it("does not while a plan runs or a card is still open", () => {
        expect(clear_is_recommended({ ...rested, running_plans: 1 })).toBe(false);
        expect(clear_is_recommended({ ...rested, open_cards: 1 })).toBe(false);
    });

    it("has nothing to say about a commander that has done nothing yet, or has no pane", () => {
        expect(clear_is_recommended({ ...rested, finished_anything: false })).toBe(false);
        expect(clear_is_recommended({ ...rested, has_pane: false })).toBe(false);
    });
});
