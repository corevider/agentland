import { describe, expect, it } from "vitest";

import { on_a_control } from "@/lib/controls";

function element(inside: string[]): { closest: (selector: string) => unknown } {
    return {
        closest: (selector) =>
            selector
                .split(",")
                .map((part) => part.trim())
                .some((part) => inside.includes(part))
                ? {}
                : null,
    };
}

describe("telling a press on a control from a press on the card", () => {
    it("leaves the card alone when the press starts on its assign menu or a button", () => {
        expect(on_a_control(element(["select"]))).toBe(true);
        expect(on_a_control(element(["button"]))).toBe(true);
        expect(on_a_control(element(["input"]))).toBe(true);
    });

    it("takes the card when the press starts on its text", () => {
        expect(on_a_control(element(["span", "article"]))).toBe(false);
    });

    it("answers no for a target that is not an element at all", () => {
        expect(on_a_control(null)).toBe(false);
        expect(on_a_control({})).toBe(false);
    });
});
