import { describe, expect, it } from "vitest";

import { place_menu, place_submenu } from "@/lib/menu_place";

const screen = { width: 1000, height: 800 };
const menu = { width: 232, height: 300 };

describe("placing a menu", () => {
    it("opens down and to the right when there is room", () => {
        expect(place_menu({ left: 100, top: 100 }, menu, screen)).toEqual({ left: 100, top: 100 });
    });

    it("flips to the left rather than covering the pointer", () => {
        const spot = place_menu({ left: 950, top: 100 }, menu, screen);

        expect(spot.left).toBe(950 - menu.width);
        expect(spot.left + menu.width).toBeLessThanOrEqual(screen.width);
    });

    it("flips upwards at the bottom edge", () => {
        const spot = place_menu({ left: 100, top: 780 }, menu, screen);

        expect(spot.top).toBe(780 - menu.height);
        expect(spot.top + menu.height).toBeLessThanOrEqual(screen.height);
    });

    it("never leaves the screen, even in a corner", () => {
        for (const at of [
            { left: 0, top: 0 },
            { left: 999, top: 799 },
            { left: 999, top: 0 },
            { left: 0, top: 799 },
        ]) {
            const spot = place_menu(at, menu, screen);

            expect(spot.left).toBeGreaterThanOrEqual(8);
            expect(spot.top).toBeGreaterThanOrEqual(8);
            expect(spot.left + menu.width).toBeLessThanOrEqual(screen.width - 8);
            expect(spot.top + menu.height).toBeLessThanOrEqual(screen.height - 8);
        }
    });

    it("keeps the top of a menu taller than the screen, rather than its bottom", () => {
        const tall = { width: 232, height: 900 };

        expect(place_menu({ left: 100, top: 400 }, tall, screen).top).toBe(8);
    });
});

describe("placing a submenu", () => {
    const row = { left: 300, right: 532, top: 200, bottom: 234 };

    it("sits beside the row it belongs to", () => {
        expect(place_submenu(row, menu, screen)).toEqual({ left: 532, top: 200 });
    });

    it("goes to the other side when the right is full", () => {
        const near_edge = { left: 700, right: 932, top: 200, bottom: 234 };

        expect(place_submenu(near_edge, menu, screen).left).toBe(700 - menu.width);
    });

    it("rises only as far as it must to keep its last row on screen", () => {
        const low = { left: 300, right: 532, top: 700, bottom: 734 };

        const spot = place_submenu(low, menu, screen);

        expect(spot.top).toBe(screen.height - menu.height - 8);
        expect(spot.top).toBeLessThan(700);
    });

    it("stays on screen when the row is in a corner", () => {
        const corner = { left: 960, right: 992, top: 780, bottom: 800 };

        const spot = place_submenu(corner, menu, screen);

        expect(spot.left).toBeGreaterThanOrEqual(8);
        expect(spot.left + menu.width).toBeLessThanOrEqual(screen.width - 8);
        expect(spot.top + menu.height).toBeLessThanOrEqual(screen.height - 8);
    });
});
