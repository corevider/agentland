import { describe, expect, it } from "vitest";

import { sideways_step, step_from_wheel } from "@/lib/wheel";

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
