import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { STEP_MS, reset_queue, upgrade_soon, waiting_count } from "@/lib/gpu_queue";

describe("handing terminals their GPU renderer", () => {
    beforeEach(() => {
        vi.useFakeTimers();
        reset_queue();
    });
    afterEach(() => vi.useRealTimers());

    it("upgrades one pane at a time rather than all at once", () => {
        const done: string[] = [];
        upgrade_soon(() => done.push("first"));
        upgrade_soon(() => done.push("second"));

        expect(done).toEqual([]);

        vi.advanceTimersByTime(STEP_MS);
        expect(done).toEqual(["first"]);

        vi.advanceTimersByTime(STEP_MS);
        expect(done).toEqual(["first", "second"]);
    });

    it("forgets a pane that closed before its turn", () => {
        const done: string[] = [];
        upgrade_soon(() => done.push("stays"));
        const cancel = upgrade_soon(() => done.push("gone"));
        cancel();

        vi.advanceTimersByTime(STEP_MS * 4);
        expect(done).toEqual(["stays"]);
        expect(waiting_count()).toBe(0);
    });

    it("starts again after the queue has run dry", () => {
        const done: string[] = [];
        upgrade_soon(() => done.push("one"));
        vi.advanceTimersByTime(STEP_MS * 3);

        upgrade_soon(() => done.push("two"));
        vi.advanceTimersByTime(STEP_MS);
        expect(done).toEqual(["one", "two"]);
    });
});
