import { describe, expect, it } from "vitest";

import { column_keeps_the_turn, place_of, sideways_step, step_from_wheel } from "@/lib/wheel";

const strip = { scroll_left: 0, scroll_width: 600, client_width: 300, delta_x: 0, delta_y: 100, delta_mode: 0 };

describe("turning a wheel into sideways scroll", () => {
    it("spends a vertical turn on a strip that has room", () => {
        expect(sideways_step(strip)).toBe(100);
    });

    it("leaves the page alone when the strip fits", () => {
        expect(sideways_step({ ...strip, scroll_width: 300 })).toBe(0);
    });

    it("stops at the end instead of over-scrolling", () => {
        expect(sideways_step({ ...strip, scroll_left: 260 })).toBe(40);
        expect(sideways_step({ ...strip, scroll_left: 300 })).toBe(0);
        expect(sideways_step({ ...strip, scroll_left: 0, delta_y: -100 })).toBe(0);
    });

    it("keeps out of the way of a trackpad already scrolling sideways", () => {
        expect(sideways_step({ ...strip, delta_x: -120, delta_y: 10 })).toBe(0);
    });

    it("reads a wheel that reports lines or pages, not pixels", () => {
        expect(step_from_wheel(3, 1)).toBe(48);
        expect(step_from_wheel(1, 2)).toBe(240);
        expect(step_from_wheel(53, 0)).toBe(53);
    });
});

describe("who gets the turn on a board", () => {
    it("lets a column with cards below the fold scroll itself", () => {
        expect(column_keeps_the_turn({ over_a_column: true, column_room: 400 })).toBe(true);
    });

    it("moves the columns sideways from the space above them", () => {
        expect(column_keeps_the_turn({ over_a_column: false, column_room: 0 })).toBe(false);
    });

    it("moves the columns sideways from a column with nothing to scroll", () => {
        expect(column_keeps_the_turn({ over_a_column: true, column_room: 0 })).toBe(false);
    });

    it("reads the place off the board's markers, and a non-element is nowhere", () => {
        const list = { scrollHeight: 900, clientHeight: 300 };
        const column = { querySelector: (selector: string) => (selector === "[data-cards]" ? list : null) };
        const inside = { closest: (selector: string) => (selector === "[data-column]" ? column : null) };

        expect(place_of(inside as unknown as Element)).toEqual({ over_a_column: true, column_room: 600 });
        expect(place_of({ closest: () => null } as unknown as Element)).toEqual({ over_a_column: false, column_room: 0 });
        expect(place_of(null)).toEqual({ over_a_column: false, column_room: 0 });
    });
});
