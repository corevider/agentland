import { describe, expect, it } from "vitest";

import {
    bytes_in_words,
    can_check,
    can_install,
    in_a_sentence,
    notes_for_reading,
    progress_line,
    type UpdateState,
} from "@/lib/updates";

const NOW = 1_700_000_000_000;

describe("what a download says about itself", () => {
    it("counts in the unit a person reads", () => {
        expect(bytes_in_words(512)).toBe("512 B");
        expect(bytes_in_words(2048)).toBe("2 KB");
        expect(bytes_in_words(48 * 1024 * 1024)).toBe("48.0 MB");
    });

    it("says a percentage only when it knows the total", () => {
        expect(progress_line(12 * 1024 * 1024, 48 * 1024 * 1024)).toBe("12.0 MB of 48.0 MB · 25%");

        // A percentage of an unknown total is a number somebody made up.
        expect(progress_line(12 * 1024 * 1024, null)).toBe("12.0 MB so far");
        expect(progress_line(12 * 1024 * 1024, 0)).toBe("12.0 MB so far");
    });

    it("does not run past a hundred when the server undercounted", () => {
        expect(progress_line(120, 100)).toContain("100%");
    });
});

describe("the sentence the panel shows", () => {
    it("says something different for each of the four waits", () => {
        const said = new Set(
            (
                [
                    { kind: "checking" },
                    { kind: "available", version: "0.2.0", notes: "", date: null },
                    { kind: "downloading", got: 1, total: 2 },
                    { kind: "ready", version: "0.2.0" },
                ] as UpdateState[]
            ).map((state) => in_a_sentence(state, NOW)),
        );

        expect(said.size).toBe(4);
    });

    it("names the version it found and the version it installed", () => {
        expect(in_a_sentence({ kind: "available", version: "0.2.0", notes: "", date: null }, NOW)).toContain(
            "0.2.0",
        );
        expect(in_a_sentence({ kind: "ready", version: "0.2.0" }, NOW)).toContain("restarts");
    });

    it("says how long ago it checked, once that stops being just now", () => {
        expect(in_a_sentence({ kind: "current", at: NOW }, NOW)).toBe("This is the newest there is.");
        expect(in_a_sentence({ kind: "current", at: NOW - 5 * 60_000 }, NOW)).toContain("5 minutes ago");
    });

    it("passes trouble through in the words it arrived in", () => {
        const said = in_a_sentence({ kind: "trouble", why: "the endpoint did not answer" }, NOW);
        expect(said).toBe("the endpoint did not answer");
    });
});

describe("which buttons are offered", () => {
    it("offers the install only when there is one to install", () => {
        expect(can_install({ kind: "available", version: "0.2.0", notes: "", date: null })).toBe(true);

        for (const state of [
            { kind: "idle" },
            { kind: "checking" },
            { kind: "current", at: NOW },
            { kind: "downloading", got: 1, total: 2 },
            { kind: "ready", version: "0.2.0" },
        ] as UpdateState[]) {
            expect(can_install(state), state.kind).toBe(false);
        }
    });

    it("does not offer a second check while one is running", () => {
        expect(can_check({ kind: "checking" })).toBe(false);
        expect(can_check({ kind: "downloading", got: 1, total: 2 })).toBe(false);

        // Nor where there is nowhere to check.
        expect(can_check({ kind: "off", why: "a development build" })).toBe(false);

        expect(can_check({ kind: "idle" })).toBe(true);
        expect(can_check({ kind: "trouble", why: "it did not answer" })).toBe(true);
    });
});

describe("release notes", () => {
    it("keeps short notes whole", () => {
        const held = notes_for_reading("one\ntwo", 10);
        expect(held).toEqual({ text: "one\ntwo", trimmed: false });
    });

    it("says when it is showing only the top", () => {
        const long = Array.from({ length: 40 }, (_, i) => `line ${i}`).join("\n");
        const held = notes_for_reading(long, 12);

        expect(held.trimmed).toBe(true);
        expect(held.text.split("\n")).toHaveLength(12);
        expect(held.text).toContain("line 0");
    });
});
