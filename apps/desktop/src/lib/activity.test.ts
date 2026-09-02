import { describe, expect, it } from "vitest";

import { families_in, family_of, meters_of, moments_ago, rule_reads, short_count } from "@/lib/activity";
import type { JournalEntry } from "@/lib/core";

const CEILINGS = { requests: 500, input: 1_000_000, output: 200_000 };

function entry(kind: string): JournalEntry {
    return { at: 0, kind, actor: "someone", subject: "", detail: "" };
}

describe("the families a journal is grouped by", () => {
    it("takes the first segment of a dotted kind", () => {
        expect(family_of("card.assigned")).toBe("card");
        expect(family_of("tick")).toBe("tick");
    });

    it("offers the most talked about first", () => {
        const held = families_in([
            entry("card.assigned"),
            entry("engine.rate_limited"),
            entry("card.moved"),
            entry("card.merged"),
            entry("engine.rate_limit_cleared"),
            entry("pull.changed"),
        ]);

        // Alphabetical would bury `card`, which is the one somebody wants.
        expect(held).toEqual(["card", "engine", "pull"]);
    });

    it("has nothing to offer for an empty journal", () => {
        expect(families_in([])).toEqual([]);
    });
});

describe("the three ceilings as bars", () => {
    it("marks the one that decides", () => {
        // A tenth of the tokens, but the requests are near the top.
        const held = meters_of({ requests: 450, input: 100_000, output: 10_000 }, CEILINGS);
        const tight = held.filter((meter) => meter.tightest);

        expect(tight).toHaveLength(1);
        expect(tight[0].label).toBe("requests");
    });

    it("keeps the rows in one order so two glances can be compared", () => {
        const busy = meters_of({ requests: 450, input: 10, output: 10 }, CEILINGS);
        const heavy = meters_of({ requests: 1, input: 900_000, output: 10 }, CEILINGS);

        expect(busy.map((m) => m.label)).toEqual(heavy.map((m) => m.label));
    });

    it("does not run a bar past its own end", () => {
        const over = meters_of({ requests: 900, input: 0, output: 0 }, CEILINGS);
        expect(over[0].share).toBe(1);
    });

    it("marks nothing when nothing has been spent", () => {
        const idle = meters_of({ requests: 0, input: 0, output: 0 }, CEILINGS);
        expect(idle.some((meter) => meter.tightest)).toBe(false);
    });
});

describe("numbers and times a person can hold", () => {
    it("shortens without lying about the scale", () => {
        expect(short_count(999)).toBe("999");
        expect(short_count(1200)).toBe("1.2k");
        expect(short_count(60_713)).toBe("61k");
        expect(short_count(1_000_000)).toBe("1.00M");
    });

    it("says how long ago in the fewest true words", () => {
        expect(moments_ago(100, 130)).toBe("30s");
        expect(moments_ago(100, 400)).toBe("5m");
        expect(moments_ago(0, 7200)).toBe("2h");
        expect(moments_ago(0, 172_800)).toBe("2d");
    });

    it("never says a negative age", () => {
        expect(moments_ago(200, 100)).toBe("0s");
    });
});

describe("rule_reads", () => {
    it("says a command rule as the command", () => {
        expect(rule_reads("Bash(npm test:*)")).toBe("run npm test…");
    });

    it("keeps an exact command exact, because the trailing star is the difference", () => {
        expect(rule_reads("Bash(npm test)")).toBe("run npm test");
    });

    it("says a folder rule as the folder, which is a different kind of permission", () => {
        expect(rule_reads("Dir(/tmp)")).toBe("reach anything under /tmp");
    });

    it("leaves a shape it does not know alone rather than mangling it", () => {
        expect(rule_reads("WebFetch(domain:example.com)")).toBe("WebFetch(domain:example.com)");
    });
});
