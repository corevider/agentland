import { describe, expect, it } from "vitest";

import type { Allowance } from "@/lib/core";
import { FIVE_HOURS } from "@/lib/logins";
import { five_hours_shown, login_label, week_shown } from "@/lib/strip";

function allowance(fields: Partial<Allowance>): Allowance {
    return {
        identity: "claude",
        agents: [],
        last_minute: {} as Allowance["last_minute"],
        ceilings: {} as Allowance["ceilings"],
        closest_to: "",
        room: "plenty",
        says: "",
        ...fields,
    };
}

describe("the strip along an agent's pane", () => {
    it("names the login the short way", () => {
        expect(login_label("claude", "second")).toBe("claude · second");
        expect(login_label("codex", null)).toBe("codex · this machine's login");
        expect(login_label("codex", "  ")).toBe("codex · this machine's login");
    });

    it("shows five hours only while the reading is younger than the window", () => {
        expect(five_hours_shown(allowance({ session_percent: 71, read_seconds_ago: 60 }))).toBe(71);
        expect(five_hours_shown(allowance({ session_percent: 99, read_seconds_ago: FIVE_HOURS }))).toBeNull();
        expect(five_hours_shown(allowance({}))).toBeNull();
        expect(five_hours_shown(null)).toBeNull();
    });

    it("shows the week whenever something has read it", () => {
        expect(week_shown(allowance({ weekly_percent: 38, read_seconds_ago: FIVE_HOURS * 3 }))).toBe(38);
        expect(week_shown(allowance({}))).toBeNull();
    });
});
