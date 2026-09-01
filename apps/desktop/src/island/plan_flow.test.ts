import { describe, expect, it } from "vitest";

import {
    color_of,
    markers_for,
    plan_to_show,
    spread_for,
    threads_for,
    type FlowStep,
} from "@/island/plan_flow";

const stations = new Map([
    ["ada", { x: 2, z: 0, rotation: 0 }],
    ["kai", { x: -2, z: 1, rotation: 0 }],
]);

const steps: FlowStep[] = [
    { id: "p1s1", title: "Serve /health", state: "done", needs: [], assignee: "ada" },
    { id: "p1s2", title: "Prove it", state: "assigned", needs: ["p1s1"], assignee: "kai" },
    { id: "p1s3", title: "Write it down", state: "waiting", needs: ["p1s2"], assignee: null },
];

describe("where a plan stands on the island", () => {
    it("gives every step a place of its own", () => {
        const markers = markers_for(steps, 5.2, stations);
        expect(markers).toHaveLength(3);
        expect(new Set(markers.map((m) => `${m.x.toFixed(2)},${m.z.toFixed(2)}`)).size).toBe(3);
    });

    it("has nothing to place when there is no plan", () => {
        expect(markers_for([], 5.2, stations)).toEqual([]);
    });

    it("keeps a single step in front of the lighthouse rather than off the island", () => {
        const [only] = markers_for([steps[0]], 5.2, stations);
        expect(Math.hypot(only.x, only.z)).toBeLessThan(5.2);
    });

    it("points a step at the station of whoever has it", () => {
        const markers = markers_for(steps, 5.2, stations);
        expect(markers[1].station).toEqual({ x: -2, z: 1 });
        expect(markers[2].station).toBeNull();
    });

    it("forgets an assignee who is no longer on the island", () => {
        const gone: FlowStep[] = [{ ...steps[1], assignee: "nobody" }];
        expect(markers_for(gone, 5.2, stations)[0].station).toBeNull();
    });
});

describe("what the plan says about itself", () => {
    it("draws a thread from a step to the one that waits for it", () => {
        const markers = markers_for(steps, 5.2, stations);
        const waits = threads_for(steps, markers).filter((t) => t.kind === "waits_for");
        expect(waits).toHaveLength(2);
    });

    it("stops drawing what a finished step was waiting for", () => {
        const finished = steps.map((step) => ({ ...step, state: "done" }));
        expect(threads_for(finished, markers_for(finished, 5.2, stations))).toEqual([]);
    });

    it("draws a thread from an assigned step to the hands it is in", () => {
        const markers = markers_for(steps, 5.2, stations);
        const handed = threads_for(steps, markers).filter((t) => t.kind === "handed_to");
        expect(handed).toHaveLength(1);
        expect(handed[0].to).toEqual({ x: -2, z: 1 });
    });
});

describe("which plan the island shows", () => {
    it("shows the one being worked", () => {
        const plans = [
            { id: "done", state: "done", steps },
            { id: "live", state: "running", steps },
        ];
        expect(plan_to_show(plans)?.id).toBe("live");
    });

    it("shows nothing when nothing is running", () => {
        expect(plan_to_show([{ id: "done", state: "done", steps }])).toBeNull();
    });
});

describe("the colour of a step", () => {
    it("says what is true of it", () => {
        expect(color_of("done")).not.toBe(color_of("waiting"));
        expect(color_of("blocked")).not.toBe(color_of("assigned"));
        expect(color_of("something new")).toBe(color_of("waiting"));
    });
});

describe("how wide the plan may stand", () => {
    it("stands at its full width in a scene with room", () => {
        expect(spread_for(900)).toBe(1);
    });

    it("closes up as the panel narrows", () => {
        expect(spread_for(400)).toBeCloseTo(400 / 620);
        expect(spread_for(200)).toBe(0.42);
    });

    it("keeps every step inside a narrow scene", () => {
        const narrow = markers_for(steps, 5.2, stations, spread_for(260));
        const wide = markers_for(steps, 5.2, stations, spread_for(1200));
        const span = (list: typeof narrow) =>
            Math.max(...list.map((m) => m.x)) - Math.min(...list.map((m) => m.x));
        expect(span(narrow)).toBeLessThan(span(wide));
    });
});

describe("labels floating clear of one another", () => {
    it("gives neighbouring steps different heights", () => {
        const markers = markers_for(
            [...steps, { id: "p1s4", title: "Review", state: "waiting", needs: [], assignee: null }],
            5.2,
            stations,
        );

        for (let index = 1; index < markers.length; index += 1) {
            expect(markers[index].lift).not.toBe(markers[index - 1].lift);
        }
    });
});
