import { describe, expect, it } from "vitest";

import type { Engine, HireableLogin, HiringReport } from "@/lib/core";
import { login_key, login_words, open_to_the_crew } from "@/lib/hiring_rules";

function engine(id: string, installed = true): Engine {
    return { id, name: id, command: id, resume: [], takes_the_tools: true, installed, version: null };
}

function login(overrides: Partial<HireableLogin> = {}): HireableLogin {
    return {
        account: null,
        open: true,
        signed_in: null,
        room: "plenty",
        session_percent: null,
        weekly_percent: null,
        ...overrides,
    };
}

function report(closed_engines: string[]): HiringReport {
    return { rules: { closed_engines, closed_logins: [], notes: {} }, engines: [] };
}

describe("login_key", () => {
    it("names the machine's own login by its engine and a second one by its label", () => {
        expect(login_key("claude", null)).toBe("claude");
        expect(login_key("codex", "work")).toBe("codex/work");
    });
});

describe("login_words", () => {
    it("says there is nothing to go on until the engine has reported", () => {
        expect(login_words(login())).toBe("no reading yet");
    });

    it("gives the week first, then the five hours, and the room when it is short", () => {
        expect(login_words(login({ weekly_percent: 84.4, session_percent: 12, room: "tight" }))).toBe(
            "week 84% · 5h 12% · tight",
        );
    });

    it("says so when nobody is signed in", () => {
        expect(login_words(login({ signed_in: false }))).toBe("nobody signed in · no reading yet");
    });
});

describe("open_to_the_crew", () => {
    it("offers what is installed and not closed", () => {
        const engines = [engine("claude"), engine("codex"), engine("gemini", false)];

        expect(open_to_the_crew(engines, report(["claude"])).map((held) => held.id)).toEqual(["codex"]);
    });

    it("offers everything installed when the rules could not be read", () => {
        expect(open_to_the_crew([engine("claude"), engine("codex")], null)).toHaveLength(2);
    });
});
