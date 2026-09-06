import { describe, expect, it } from "vitest";

import { chief_of, clear_is_recommended, commander_of } from "@/lib/commander";

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

describe("who commands what", () => {
    const crew = [
        { role: "chief", repository_id: "", workspace_id: "ws1", name: "X" },
        { role: "chief", repository_id: "", workspace_id: "ws2", name: "X" },
        { role: "commander", repository_id: "svc-demo", workspace_id: null, name: "X" },
        { role: "implementer", repository_id: "svc-demo", workspace_id: null, name: "Ada" },
    ];

    it("finds the chief of the workspace you are standing in, not the first one", () => {
        expect(chief_of(crew, "ws2")).toBe(crew[1]);
        expect(chief_of(crew, "ws1")).toBe(crew[0]);
    });

    it("says nobody when there is nowhere to stand", () => {
        expect(chief_of(crew, null)).toBeUndefined();
        expect(chief_of(crew, "ws9")).toBeUndefined();
    });

    it("finds a project's commander and never its chief", () => {
        expect(commander_of(crew, "svc-demo")).toBe(crew[2]);
        expect(commander_of(crew, "")).toBeUndefined();
    });
});
