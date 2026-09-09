import { describe, expect, it } from "vitest";

import { takes_dictation, typed_into } from "@/lib/dictation";

describe("what can be spoken into", () => {
    it("takes a box that holds a sentence", () => {
        expect(takes_dictation({ tag: "TEXTAREA" })).toBe(true);
        expect(takes_dictation({ tag: "INPUT", type: null })).toBe(true);
        expect(takes_dictation({ tag: "input", type: "text" })).toBe(true);
        expect(takes_dictation({ tag: "INPUT", type: "search" })).toBe(true);
    });

    it("leaves alone a box that holds something else", () => {
        expect(takes_dictation({ tag: "INPUT", type: "checkbox" })).toBe(false);
        expect(takes_dictation({ tag: "INPUT", type: "number" })).toBe(false);
        expect(takes_dictation({ tag: "SELECT" })).toBe(false);
        expect(takes_dictation({ tag: "DIV" })).toBe(false);
        expect(takes_dictation(null)).toBe(false);
    });

    it("does not write a spoken secret into a password box", () => {
        expect(takes_dictation({ tag: "INPUT", type: "password" })).toBe(false);
    });

    it("does not write into a box nobody can type in", () => {
        expect(takes_dictation({ tag: "TEXTAREA", readonly: true })).toBe(false);
        expect(takes_dictation({ tag: "INPUT", type: "text", disabled: true })).toBe(false);
    });

    it("leaves a terminal to the pane it already had", () => {
        // xterm keeps a hidden textarea to catch keystrokes. Writing into that
        // types into nothing — a pane is written to through the core.
        expect(takes_dictation({ tag: "TEXTAREA", in_terminal: true })).toBe(false);
    });
});

describe("what a spoken sentence does to the box it lands in", () => {
    it("goes in at the caret", () => {
        expect(typed_into({ value: "read the board", start: 5, end: 5 }, "over")).toEqual({
            value: "read over the board",
            caret: 9,
        });
    });

    it("replaces what was selected, the way typing would", () => {
        expect(typed_into({ value: "read the board", start: 5, end: 8 }, "every")).toEqual({
            value: "read every board",
            caret: 10,
        });
    });

    it("does not run a sentence into the word before it", () => {
        const put = typed_into({ value: "fix the", start: 7, end: 7 }, "auth bug");

        expect(put.value).toBe("fix the auth bug");
        expect(put.caret).toBe(16);
    });

    it("adds no space where there is already one, or nothing at all", () => {
        expect(typed_into({ value: "fix the ", start: 8, end: 8 }, "bug").value).toBe("fix the bug");
        expect(typed_into({ value: "", start: 0, end: 0 }, "bug").value).toBe("bug");
        expect(typed_into({ value: "(", start: 1, end: 1 }, "bug").value).toBe("(bug");
    });

    it("leaves the box as it was when nothing was said", () => {
        expect(typed_into({ value: "held", start: 2, end: 4 }, "   ")).toEqual({
            value: "held",
            caret: 4,
        });
    });

    it("survives a caret the box no longer has", () => {
        expect(typed_into({ value: "short", start: 99, end: 99 }, "word")).toEqual({
            value: "short word",
            caret: 10,
        });
    });
});
