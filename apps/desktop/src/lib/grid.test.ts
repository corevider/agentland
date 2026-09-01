import { describe, expect, it } from "vitest";

import { MOST_PANES, SMALLEST_TRACK, best_columns, edge, fits_readably, grid_shape, page_count, page_of, resize_tracks, to_template, tracks_for } from "@/lib/grid";

describe("how many terminals a panel shows", () => {
    it("shows them all while they fit", () => {
        expect(page_of([1, 2, 3], 0)).toEqual([1, 2, 3]);
        expect(page_count(3)).toBe(1);
    });

    it("stops at eight and puts the rest on another page", () => {
        const eleven = Array.from({ length: 11 }, (_, index) => index);
        expect(page_of(eleven, 0)).toHaveLength(MOST_PANES);
        expect(page_of(eleven, 1)).toEqual([8, 9, 10]);
        expect(page_count(11)).toBe(2);
    });

    it("keeps a page that no longer exists in range", () => {
        expect(page_of([1, 2], 5)).toEqual([1, 2]);
    });
});

describe("the shape of the grid", () => {
    it("asks for no more columns than there are panes", () => {
        expect(grid_shape(2, 4)).toEqual({ columns: 2, rows: 1 });
        expect(grid_shape(5, 3)).toEqual({ columns: 3, rows: 2 });
        expect(grid_shape(8, 4)).toEqual({ columns: 4, rows: 2 });
    });

    it("has a shape even with nothing in it", () => {
        expect(grid_shape(0, 4)).toEqual({ columns: 1, rows: 1 });
    });
});

describe("sizing one terminal against its neighbour", () => {
    it("starts even", () => {
        expect(tracks_for(3)).toEqual([1, 1, 1]);
    });

    it("keeps sizes that still fit the grid and drops ones that do not", () => {
        expect(tracks_for(2, [1.4, 0.6])).toEqual([1.4, 0.6]);
        expect(tracks_for(3, [1.4, 0.6])).toEqual([1, 1, 1]);
    });

    it("takes from one side and gives to the other, leaving the rest alone", () => {
        const tracks = resize_tracks([1, 1, 1], 0, 0.75);
        expect(tracks[0] + tracks[1]).toBeCloseTo(2);
        expect(tracks[0]).toBeCloseTo(1.5);
        expect(tracks[2]).toBe(1);
    });

    it("never squeezes a terminal out of existence", () => {
        expect(resize_tracks([1, 1], 0, 0)[0]).toBe(SMALLEST_TRACK);
        expect(resize_tracks([1, 1], 0, 1)[1]).toBe(SMALLEST_TRACK);
    });

    it("ignores a gap that is not between two tracks", () => {
        expect(resize_tracks([1, 1], 1, 0.5)).toEqual([1, 1]);
        expect(resize_tracks([1, 1], -1, 0.5)).toEqual([1, 1]);
    });

    it("writes what CSS grid wants", () => {
        expect(to_template([1.5, 0.5])).toBe("1.500fr 0.500fr");
    });
});

describe("where a drag handle sits", () => {
    it("sits on the gap between two even tracks", () => {
        expect(edge([1, 1], 0)).toBeCloseTo(0.5);
        expect(edge([1, 1, 1], 0)).toBeCloseTo(1 / 3);
        expect(edge([1, 1, 1], 1)).toBeCloseTo(2 / 3);
    });

    it("follows a track that has been made bigger", () => {
        expect(edge([1.5, 0.5], 0)).toBeCloseTo(0.75);
    });

    it("has somewhere to be even with an empty grid", () => {
        expect(edge([], 0)).toBe(0);
    });
});

describe("choosing an arrangement for the space there is", () => {
    it("puts eight terminals side by side in a wide, short panel", () => {
        expect(best_columns(8, 1600, 400)).toBe(4);
    });

    it("stacks them in a narrow, tall one", () => {
        expect(best_columns(8, 420, 1200)).toBe(1);
    });

    it("squares them up when the panel is square", () => {
        expect(best_columns(4, 900, 700)).toBe(2);
    });

    it("has nothing to arrange with one terminal", () => {
        expect(best_columns(1, 1600, 900)).toBe(1);
        expect(best_columns(8, 0, 0)).toBe(1);
    });
});

describe("how many fit before they stop being readable", () => {
    it("shows eight when there is room for eight", () => {
        expect(fits_readably(1600, 900)).toBe(8);
    });

    it("shows fewer in a panel that is one sixth of a window", () => {
        expect(fits_readably(400, 370)).toBe(2);
        expect(fits_readably(760, 460)).toBe(6);
    });

    it("never shows nothing", () => {
        expect(fits_readably(100, 80)).toBe(1);
    });

    it("assumes the ceiling before the panel has been measured", () => {
        expect(fits_readably(0, 0)).toBe(8);
    });
});
