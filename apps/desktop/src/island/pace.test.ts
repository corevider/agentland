import { describe, expect, it } from "vitest";

import { PACE, SHARE, SLOWEST_MS, frame_target, frame_wait } from "@/island/pace";

const watching = { hidden: false, showing: true, focused: true, interacting: false, moving: false };

describe("what the island is worth drawing at", () => {
    it("draws nothing behind another tab or a hidden window", () => {
        expect(frame_target({ ...watching, showing: false })).toBe(0);
        expect(frame_target({ ...watching, hidden: true })).toBe(0);
        expect(frame_target({ ...watching, hidden: true, interacting: true, moving: true })).toBe(0);
    });

    it("asks for nothing while nothing on it moves", () => {
        expect(frame_target(watching)).toBe(0);
    });

    it("keeps up while a crew member is working or a hand-off is flying", () => {
        expect(frame_target({ ...watching, moving: true })).toBe(PACE.moving);
    });

    it("keeps up while it is being dragged, even with a still crew", () => {
        expect(frame_target({ ...watching, interacting: true })).toBe(PACE.interacting);
    });

    it("barely ticks over when the work is elsewhere", () => {
        expect(frame_target({ ...watching, moving: true, focused: false })).toBe(PACE.moving_away);
        expect(frame_target({ ...watching, focused: false })).toBe(0);
    });
});

describe("what the machine can afford", () => {
    it("keeps the asked-for pace where a frame is cheap", () => {
        expect(frame_wait(24, 4)).toBeCloseTo(1000 / 24);
    });

    it("slows down where a frame is expensive, rather than saturating a core", () => {
        // Measured here: WebKitGTK without a GPU draws the island in ~40 ms.
        expect(frame_wait(24, 40)).toBe(40 / SHARE);
        expect(1000 / frame_wait(24, 40)).toBeCloseTo(6.25);
    });

    it("still looks alive on a machine that cannot keep up at all", () => {
        // A first frame carrying shader compilation once measured half a second.
        expect(frame_wait(24, 500)).toBe(SLOWEST_MS);
    });

    it("waits for nothing when nothing is being drawn", () => {
        expect(frame_wait(0, 40)).toBe(0);
    });

    it("takes the asked-for pace until a frame has been measured", () => {
        expect(frame_wait(24, 0)).toBeCloseTo(1000 / 24);
    });
});
