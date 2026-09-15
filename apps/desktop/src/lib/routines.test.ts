import { describe, expect, it } from "vitest";

import type { Routine, RoutineTemplate } from "@/lib/core";
import {
    EMPTY_DRAFT,
    WEEKDAYS,
    days_in_words,
    draft_from_routine,
    draft_from_template,
    draft_problem,
    insert_at,
    payload_of,
    schedule_in_words,
} from "@/lib/routines";

function routine(overrides: Partial<Routine>): Routine {
    return {
        id: "r1",
        name: "sweep",
        agent_id: "x",
        brief: "look around",
        schedule: { kind: "every", minutes: 60, days: [] },
        delivery: "card",
        draft_only: false,
        skip_when_tight: true,
        one_at_a_time: true,
        pause_after_failures: 2,
        enabled: true,
        created_by: null,
        created_at: 0,
        last_run: 0,
        consecutive_failures: 0,
        last_result: null,
        last_card: null,
        waiting_since: 0,
        history: [],
        next_run: null,
        ...overrides,
    };
}

describe("saying a schedule", () => {
    it("says set times the way a person would", () => {
        expect(schedule_in_words({ kind: "daily", times: ["09:00"], days: [...WEEKDAYS] })).toBe("weekdays at 09:00");
        expect(schedule_in_words({ kind: "daily", times: ["02:30"], days: [] })).toBe("daily at 02:30");
        expect(schedule_in_words({ kind: "daily", times: ["17:00"], days: ["fri"] })).toBe("Fridays at 17:00");
        expect(schedule_in_words({ kind: "daily", times: ["10:00", "16:00"], days: [...WEEKDAYS] })).toBe(
            "weekdays at 10:00 and 16:00",
        );
        expect(schedule_in_words({ kind: "daily", times: ["08:00", "12:00", "18:00"], days: ["sat", "sun"] })).toBe(
            "weekends at 08:00, 12:00 and 18:00",
        );
    });

    it("says an interval with its window and its days", () => {
        expect(schedule_in_words({ kind: "every", minutes: 30, days: [] })).toBe("every 30 min");
        expect(schedule_in_words({ kind: "every", minutes: 60, days: [] })).toBe("every hour");
        expect(schedule_in_words({ kind: "every", minutes: 90, days: [] })).toBe("every 90 min");
        expect(
            schedule_in_words({ kind: "every", minutes: 240, from: "09:00", to: "19:00", days: [...WEEKDAYS] }),
        ).toBe("every 4 hours, 09:00–19:00, weekdays");
    });

    it("says days as a range, a list, or not at all", () => {
        expect(days_in_words([])).toBeNull();
        expect(days_in_words(["mon", "tue", "wed", "thu", "fri", "sat", "sun"])).toBeNull();
        expect(days_in_words(["thu", "mon", "wed", "tue"])).toBe("Mon–Thu");
        expect(days_in_words(["wed", "mon"])).toBe("Mon, Wed");
    });
});

describe("drafting a routine", () => {
    it("refuses in words that say what to fix", () => {
        const ready = { ...EMPTY_DRAFT, name: "triage", agent_id: "ada", brief: "read the night" };
        expect(draft_problem(ready)).toBeNull();

        expect(draft_problem({ ...ready, name: " " })).toContain("name");
        expect(draft_problem({ ...ready, agent_id: "" })).toContain("agent");
        expect(draft_problem({ ...ready, times: ["9am"] })).toContain("HH:MM");
        expect(draft_problem({ ...ready, times: [" "] })).toContain("at least one time");
        expect(draft_problem({ ...ready, kind: "every", windowed: true, to: "" })).toContain("both ends");
    });

    it("sends the schedule the core keeps: padded, sorted, each time once", () => {
        const payload = payload_of({
            ...EMPTY_DRAFT,
            name: " triage ",
            agent_id: "ada",
            brief: "read",
            times: ["16:00", "9:00", "16:00", ""],
            days: ["fri", "mon"],
        });

        expect(payload.name).toBe("triage");
        expect(payload.schedule).toEqual({ kind: "daily", times: ["09:00", "16:00"], days: ["mon", "fri"] });
    });

    it("sends no window when the interval has none", () => {
        const payload = payload_of({ ...EMPTY_DRAFT, kind: "every", minutes: 45, windowed: false });
        expect(payload.schedule).toEqual({ kind: "every", minutes: 45, from: null, to: null, days: [...WEEKDAYS] });
    });

    it("opens a routine for editing as it was saved", () => {
        const draft = draft_from_routine(
            routine({ schedule: { kind: "every", minutes: 240, from: "09:00", to: "19:00", days: ["mon"] } }),
        );

        expect(draft.kind).toBe("every");
        expect(draft.windowed).toBe(true);
        expect(payload_of(draft).schedule).toEqual({
            kind: "every",
            minutes: 240,
            from: "09:00",
            to: "19:00",
            days: ["mon"],
        });
    });

    it("keeps the agent when a template is laid over the draft", () => {
        const template: RoutineTemplate = {
            id: "weekly-recap",
            name: "Weekly recap",
            summary: "",
            suits: "commander",
            brief: "recap the week ending {date}",
            schedule: { kind: "daily", times: ["17:00"], days: ["fri"] },
            delivery: "pane",
            draft_only: false,
            skip_when_tight: true,
            one_at_a_time: true,
        };

        const draft = draft_from_template(template, { ...EMPTY_DRAFT, agent_id: "x" });
        expect(draft.agent_id).toBe("x");
        expect(draft.delivery).toBe("pane");
        expect(draft.times).toEqual(["17:00"]);
    });

    it("puts a variable where the caret is", () => {
        expect(insert_at("It is  now.", 6, 6, "date")).toEqual({ text: "It is {date} now.", caret: 12 });
        expect(insert_at("replace me", 0, 7, "agent")).toEqual({ text: "{agent} me", caret: 7 });
    });
});
