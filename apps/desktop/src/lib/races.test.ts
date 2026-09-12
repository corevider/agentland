import { describe, expect, it } from "vitest";

import type { Engine, Race, Task } from "@/lib/core";
import { another_lane, first_lanes, lane_words, race_on, why_not_race } from "@/lib/races";

const engine = (id: string, installed = true): Engine => ({
    id,
    name: id,
    command: id,
    resume: [],
    takes_the_tools: true,
    installed,
    version: null,
});

const card: Task = {
    id: "t12",
    title: "Add /health",
    body: "",
    column: "backlog",
    repository_id: "shop",
    assignee: null,
    worktree: null,
    branch: null,
    evidence: [],
};

const race = (id: string, ended_at: number | null): Race => ({
    id,
    task_id: "t12",
    repository_id: "shop",
    entrants: [],
    started_at: 10,
    winner: null,
    ended_at,
});

describe("starting a race", () => {
    it("puts two engines against each other when two are installed", () => {
        expect(first_lanes([engine("claude"), engine("gemini", false), engine("codex")])).toEqual([
            { engine_id: "claude", model: "opus" },
            { engine_id: "codex", model: "" },
        ]);
    });

    it("puts one engine's models against each other when it is the only one", () => {
        expect(first_lanes([engine("claude"), engine("codex", false)])).toEqual([
            { engine_id: "claude", model: "opus" },
            { engine_id: "claude", model: "sonnet" },
        ]);
    });

    it("offers nothing with nothing installed", () => {
        expect(first_lanes([engine("claude", false)])).toEqual([]);
    });

    it("adds a lane nobody is running yet", () => {
        const engines = [engine("claude"), engine("codex")];

        expect(another_lane([{ engine_id: "claude", model: "opus" }], engines)).toEqual({
            engine_id: "claude",
            model: "sonnet",
        });
        expect(
            another_lane(
                [
                    { engine_id: "claude", model: "opus" },
                    { engine_id: "claude", model: "sonnet" },
                    { engine_id: "claude", model: "haiku" },
                ],
                engines,
            ),
        ).toEqual({ engine_id: "codex", model: "" });
    });

    it("says why a card cannot be raced", () => {
        expect(why_not_race(card, null)).toBeNull();
        expect(why_not_race({ ...card, assignee: "ada" }, null)).toBe("ada holds it");
        expect(why_not_race({ ...card, worktree: "tidy" }, null)).toBe("it is bound to the tidy worktree");
        expect(why_not_race({ ...card, column: "review" }, null)).toBe("it is already in review");
        expect(why_not_race(card, race("r3", null))).toBe("already racing in r3");
    });
});

describe("a race on the board", () => {
    it("finds the race still running on a card, not one already over", () => {
        expect(race_on([race("r1", 20), race("r2", null)], "t12")?.id).toBe("r2");
        expect(race_on([race("r1", 20)], "t12")).toBeNull();
        expect(race_on([race("r2", null)], "t13")).toBeNull();
    });

    it("names a lane by engine and model", () => {
        expect(lane_words({ engine_id: "claude", model: "opus" })).toBe("claude · opus");
        expect(lane_words({ engine_id: "codex", model: " " })).toBe("codex");
        expect(lane_words({ engine_id: "codex", model: null })).toBe("codex");
    });
});
