import { describe, expect, it } from "vitest";

import { first_pickable, matches, narrow, shown, step, type Choice } from "@/lib/picking";

const crew: Choice[] = [
    { value: "ada", label: "Ada", hint: "agentland/ege" },
    { value: "dag", label: "Dag", hint: "agentland-svc-demo/x-desk" },
    { value: "pax", label: "Pax", hint: "agentland/ege" },
    { value: "adalet", label: "Adalet", hint: "errands/main" },
];

describe("what typing narrows a list to", () => {
    it("leaves the order alone when nothing is typed", () => {
        expect(narrow(crew, "").map((held) => held.value)).toEqual(["ada", "dag", "pax", "adalet"]);
        expect(narrow(crew, "   ").map((held) => held.value)).toEqual(["ada", "dag", "pax", "adalet"]);
    });

    it("puts what was typed exactly above what merely starts with it", () => {
        expect(narrow(crew, "ada").map((held) => held.value)).toEqual(["ada", "adalet"]);
    });

    it("finds a row by the quiet half of it, below one found by its name", () => {
        expect(narrow(crew, "errands").map((held) => held.value)).toEqual(["adalet"]);
        expect(matches(crew[3], "errands")).toBeLessThan(matches(crew[3], "adalet"));
    });

    it("takes words in any order, and every one has to land", () => {
        expect(narrow(crew, "ada agentland").map((held) => held.value)).toEqual(["ada"]);
        expect(narrow(crew, "agentland ada").map((held) => held.value)).toEqual(["ada"]);
        expect(narrow(crew, "ada nowhere")).toEqual([]);
    });

    it("keeps the given order between rows that answer equally well", () => {
        expect(narrow(crew, "a").map((held) => held.value)).toEqual(["ada", "adalet", "dag", "pax"]);
    });

    it("does not care about case", () => {
        expect(narrow(crew, "PAX").map((held) => held.value)).toEqual(["pax"]);
    });
});

describe("where the highlight goes", () => {
    const held: Choice[] = [
        { value: "one", label: "One" },
        { value: "two", label: "Two", disabled: true },
        { value: "three", label: "Three" },
    ];

    it("steps over what cannot be picked", () => {
        expect(step(held, 0, 1)).toBe(2);
        expect(step(held, 2, -1)).toBe(0);
    });

    it("stops at the ends rather than wrapping past the answer", () => {
        expect(step(held, 2, 1)).toBe(2);
        expect(step(held, 0, -1)).toBe(0);
    });

    it("has nowhere to go in an empty list", () => {
        expect(step([], 0, 1)).toBe(-1);
        expect(first_pickable([])).toBe(-1);
    });

    it("starts on the first row anybody can pick", () => {
        expect(first_pickable(held)).toBe(0);
        expect(first_pickable([{ value: "x", label: "X", disabled: true }, ...held])).toBe(1);
    });
});

describe("what the closed box says", () => {
    it("is the label of what is picked", () => {
        expect(shown(crew, "pax", "pick one")).toBe("Pax");
    });

    it("falls back when what was picked is not in the list any more", () => {
        expect(shown(crew, "somebody-let-go", "pick one")).toBe("pick one");
        expect(shown(crew, "", "pick one")).toBe("pick one");
    });
});
