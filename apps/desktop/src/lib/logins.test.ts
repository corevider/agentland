import { describe, expect, it } from "vitest";

import type { AccountsReport, Agent, Allowance, JournalEntry } from "@/lib/core";
import {
    FIVE_HOURS,
    fallback_rows,
    five_hours_words,
    with_fallback,
    without_fallback,
    hand_overs,
    login_rows,
    nearness,
    ordinal,
    out_words,
    rank_among,
    reorder,
    wait_words,
    week_words,
    who_spends,
} from "@/lib/logins";

describe("a login's five hours", () => {
    const read = (session_percent: number | undefined, read_seconds_ago: number) =>
        ({ ...allowance("claude", []), session_percent, read_seconds_ago }) as Allowance;

    it("says how much of the five hours is gone", () => {
        expect(five_hours_words(read(71.4, 60))).toBe("71% of its five hours");
    });

    it("says nothing is known before a pane has read it", () => {
        expect(five_hours_words(read(undefined, 0))).toBe("five hours not read yet");
        expect(five_hours_words(null)).toBe("five hours not read yet");
    });

    it("does not show a login as full on a reading older than the window", () => {
        expect(five_hours_words(read(99, FIVE_HOURS))).toContain("come round");
        expect(five_hours_words(read(99, FIVE_HOURS - 1))).toBe("99% of its five hours");
    });

    it("says when a login its engine said is out comes back, and on which wall", () => {
        const out = (limit_window: Allowance["limit_window"], back_in: number) =>
            ({ ...allowance("claude", []), limit_back_at: 10_000 + back_in, limit_window }) as Allowance;

        expect(out_words(out("session", 72 * 60), 10_000)).toBe("out on its five hours — back in 1h 12m");
        expect(out_words(out("weekly", 2 * 86_400 + 3 * 3600), 10_000)).toBe("out on its week — back in 2d 3h");
        expect(out_words(out("unknown", 30), 10_000)).toBe("out — back in under a minute");
        expect(out_words(out("session", 0), 10_000)).toBeNull();
        expect(out_words(allowance("claude", []), 10_000)).toBeNull();
    });

    it("keeps a model's fallback the way a limit line names the model, and never a model as its own", () => {
        expect(with_fallback({}, " Fable ", " opus ")).toEqual({ fable: "opus" });
        expect(with_fallback({ fable: "opus" }, "fable", "sonnet")).toEqual({ fable: "sonnet" });
        expect(with_fallback({}, "opus", "Opus")).toEqual({});
        expect(with_fallback({}, "", "opus")).toEqual({});
        expect(without_fallback({ fable: "opus", opus: "sonnet" }, "fable")).toEqual({ opus: "sonnet" });
        expect(fallback_rows({ opus: "sonnet", fable: "opus" })).toEqual([
            { from: "fable", to: "opus" },
            { from: "opus", to: "sonnet" },
        ]);
    });

    it("says a wait the way a person would", () => {
        expect(wait_words(59)).toBe("under a minute");
        expect(wait_words(45 * 60)).toBe("45m");
        expect(wait_words(3 * 3600)).toBe("3h");
        expect(wait_words(86_400)).toBe("1d");
    });

    it("colours a bar by how near it is to the point agents are moved on at", () => {
        expect(nearness(96, 95)).toBe("spent");
        expect(nearness(85, 95)).toBe("tight");
        expect(nearness(40, 95)).toBe("plenty");
        expect(nearness(undefined, 95)).toBe("plenty");
    });
});

function allowance(identity: string, agents: string[], weekly_percent?: number): Allowance {
    return {
        identity,
        agents,
        weekly_percent,
        last_minute: {} as Allowance["last_minute"],
        ceilings: {} as Allowance["ceilings"],
        closest_to: "",
        room: "plenty",
        says: "",
    };
}

const report: AccountsReport = {
    accounts: [
        { engine_id: "claude", label: "second", signed_in: true, who: "second@example.com", plan: "max", askable: true },
        { engine_id: "codex", label: "work", signed_in: true, who: "work@example.com", plan: "pro", askable: true },
    ],
    engines: [
        { id: "claude", name: "Claude Code" },
        { id: "codex", name: "Codex" },
    ],
    failover: true,
};

const crew = [
    { id: "ada", name: "Ada" },
    { id: "kai", name: "Kai" },
    { id: "mira", name: "Mira" },
] as Agent[];

describe("the logins and who spends from them", () => {
    it("puts each engine's own login above the ones added here until told otherwise", () => {
        const rows = login_rows(
            report,
            [allowance("claude", ["mira"], 96), allowance("claude/second", ["ada", "kai"], 31)],
            crew,
        );

        expect(rows.map((row) => [row.identity, row.agents])).toEqual([
            ["claude", ["Mira"]],
            ["claude/second", ["Ada", "Kai"]],
            ["codex", []],
            ["codex/work", []],
        ]);
        expect(rows[0].account).toBeNull();
        expect(rows[1].account?.plan).toBe("max");
    });

    it("keeps each engine's logins in the order the person gave", () => {
        const rows = login_rows({ ...report, order: ["claude/second", "claude", "codex/work", "codex"] }, [], crew);
        expect(rows.map((row) => row.identity)).toEqual(["claude/second", "claude", "codex/work", "codex"]);
    });

    it("says an agent id nobody answers to as it is", () => {
        const rows = login_rows(report, [allowance("claude/second", ["gone"])], crew);
        expect(rows.find((row) => row.label === "second")?.agents).toEqual(["gone"]);
    });

    it("moves a login a place within its engine and never across to another", () => {
        const rows = login_rows(report, [], crew);

        expect(reorder(rows, "claude/second", -1)).toEqual(["claude/second", "claude", "codex", "codex/work"]);
        expect(reorder(rows, "claude/second", 1)).toEqual(["claude", "claude/second", "codex", "codex/work"]);
        expect(reorder(rows, "claude", -1)).toEqual(["claude", "claude/second", "codex", "codex/work"]);
    });

    it("says where a login stands among its engine's", () => {
        const rows = login_rows(report, [], crew);
        expect(rank_among(rows, rows[1])).toEqual({ rank: 2, of: 2 });
        expect(ordinal(1)).toBe("1st");
        expect(ordinal(2)).toBe("2nd");
        expect(ordinal(3)).toBe("3rd");
        expect(ordinal(11)).toBe("11th");
    });

    it("says how much of the week is gone, or that nobody has read it yet", () => {
        expect(week_words(allowance("claude", [], 95.6))).toBe("96% of the week spent");
        expect(week_words(allowance("claude", []))).toContain("not read yet");
        expect(week_words(null)).toContain("not read yet");
    });

    it("names who is on a login", () => {
        expect(who_spends([])).toBe("nobody on it now");
        expect(who_spends(["Ada"])).toBe("Ada spends from it");
        expect(who_spends(["Ada", "Kai", "Tor"])).toBe("Ada, Kai and Tor spend from it");
    });

    it("keeps the latest hand-overs, newest first", () => {
        const entries: JournalEntry[] = [
            { at: 10, kind: "accounts.handed_over", actor: "", subject: "ada", detail: "first" },
            { at: 30, kind: "card.moved", actor: "", subject: "t1", detail: "noise" },
            { at: 20, kind: "accounts.handed_over", actor: "", subject: "kai", detail: "second" },
        ];

        expect(hand_overs(entries).map((entry) => entry.detail)).toEqual(["second", "first"]);
    });
});
