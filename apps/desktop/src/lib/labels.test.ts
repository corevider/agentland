import { describe, expect, it } from "vitest";

import { spread_labels, type Label } from "@/lib/labels";

const canvas = { width: 400, height: 300 };

function tag(id: string, x: number, y: number, width = 90, height = 18): Label {
    return { id, x, y, width, height };
}

describe("keeping name tags apart", () => {
    it("leaves labels that do not touch where they are", () => {
        const spots = spread_labels([tag("a", 60, 100), tag("b", 300, 250)], canvas);

        expect(spots.get("a")).toEqual({ x: 60, y: 100 });
        expect(spots.get("b")).toEqual({ x: 300, y: 250 });
    });

    it("lifts the one behind when two land on each other", () => {
        const spots = spread_labels([tag("front", 100, 200), tag("behind", 110, 195)], canvas);

        expect(spots.get("front")).toEqual({ x: 100, y: 200 });
        expect(spots.get("behind")!.y).toBeLessThanOrEqual(200 - 18 - 3);
    });

    it("keeps the nearest station's tag where it belongs", () => {
        const spots = spread_labels([tag("far", 100, 150), tag("near", 100, 152)], canvas);

        expect(spots.get("near")!.y).toBe(152);
        expect(spots.get("far")!.y).toBeLessThan(150);
    });

    it("stacks three in a heap without any pair still touching", () => {
        const heap = [tag("a", 100, 200), tag("b", 105, 198), tag("c", 95, 196)];

        const spots = spread_labels(heap, canvas);
        const boxes = heap.map((label) => ({ ...label, ...spots.get(label.id)! }));

        for (let i = 0; i < boxes.length; i += 1) {
            for (let j = i + 1; j < boxes.length; j += 1) {
                const sideways = Math.abs(boxes[i].x - boxes[j].x) >= (boxes[i].width + boxes[j].width) / 2 + 3;
                const stacked =
                    boxes[i].y - boxes[i].height >= boxes[j].y + 3 ||
                    boxes[j].y - boxes[j].height >= boxes[i].y + 3;

                expect(sideways || stacked).toBe(true);
            }
        }
    });

    it("does not lift a label off the top of the canvas", () => {
        const spots = spread_labels([tag("a", 100, 20), tag("b", 100, 22)], canvas);

        for (const spot of spots.values()) {
            expect(spot.y).toBeGreaterThanOrEqual(18);
        }
    });

    it("leaves labels alone when they are far apart sideways", () => {
        const spots = spread_labels([tag("a", 50, 200), tag("b", 350, 200)], canvas);

        expect(spots.get("a")!.y).toBe(200);
        expect(spots.get("b")!.y).toBe(200);
    });
});
