import { afterEach, describe, expect, it, vi } from "vitest";
import { edge_scroll_step, follow_board_edge } from "./board_drag";

const strip = {
    left: 100, right: 700, top: 80, bottom: 500,
    scroll_left: 300, scroll_width: 1500, client_width: 600,
};

afterEach(() => vi.unstubAllGlobals());

describe("scrolling while carrying a board card", () => {
    it("scrolls both ways and gets faster nearer the edge", () => {
        expect(edge_scroll_step(strip, { x: 100, y: 200 }, 20)).toBeCloseTo(-14.4);
        expect(edge_scroll_step(strip, { x: 700, y: 200 }, 20)).toBeCloseTo(14.4);
        expect(edge_scroll_step(strip, { x: 664, y: 200 }, 20)).toBeCloseTo(7.2);
    });

    it("stays still in the middle, over the detail panel, and outside the board", () => {
        for (const point of [{ x: 400, y: 200 }, { x: 701, y: 200 }, { x: 99, y: 200 }, { x: 690, y: 79 }, { x: 690, y: 501 }]) {
            expect(edge_scroll_step(strip, point, 20)).toBe(0);
        }
    });

    it("clamps at both ends and does nothing when all columns fit", () => {
        expect(edge_scroll_step({ ...strip, scroll_left: 895 }, { x: 700, y: 200 }, 20)).toBe(5);
        expect(edge_scroll_step({ ...strip, scroll_left: 5 }, { x: 100, y: 200 }, 20)).toBe(-5);
        expect(edge_scroll_step({ ...strip, scroll_left: 0, scroll_width: 600 }, { x: 700, y: 200 }, 20)).toBe(0);
    });

    it("uses elapsed time but prevents a large jump after a suspended frame", () => {
        const point = { x: 700, y: 200 };
        expect(edge_scroll_step(strip, point, 32)).toBeCloseTo(2 * edge_scroll_step(strip, point, 16));
        expect(edge_scroll_step(strip, point, 5000)).toBeCloseTo(36);
    });

    it("keeps scrolling under a stationary pointer, re-aims, and stops on cleanup", () => {
        const frames = new Map<number, FrameRequestCallback>();
        let next = 0;
        vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
            frames.set(++next, callback);
            return next;
        });
        vi.stubGlobal("cancelAnimationFrame", (id: number) => frames.delete(id));
        const tick = (now: number) => {
            const pending = [...frames.values()];
            frames.clear();
            pending.forEach((callback) => callback(now));
        };
        const element = {
            scrollLeft: 300, scrollWidth: 1500, clientWidth: 600,
            getBoundingClientRect: () => strip,
        } as unknown as HTMLElement;
        let point = { x: 695, y: 200 };
        const aimed = vi.fn();
        const stop = follow_board_edge(element, () => point, aimed);
        tick(0);
        tick(16);
        const first = element.scrollLeft;
        tick(32);
        expect(element.scrollLeft).toBeGreaterThan(first);
        expect(aimed).toHaveBeenCalledTimes(2);
        expect(aimed).toHaveBeenLastCalledWith(point);
        point = { x: 400, y: 200 };
        const held = element.scrollLeft;
        tick(48);
        expect(element.scrollLeft).toBe(held);
        stop();
        expect(frames.size).toBe(0);
        tick(64);
        expect(element.scrollLeft).toBe(held);
    });
});
