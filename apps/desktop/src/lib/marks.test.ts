import { describe, expect, it } from "vitest";

import type { Attachment, Mark } from "@/lib/core";
import {
    badge_circle,
    badge_point,
    badge_under,
    derived_name,
    is_worth_keeping,
    marked_copy_of,
    normalized_box,
    originals,
} from "@/lib/marks";

const picture: Attachment = { name: "shot.png", path: "/a/shot.png", kind: "image/png", bytes: 1, at: 0 };
const copy: Attachment = { ...picture, name: "shot.marked.png", derived_from: "shot.png" };

describe("marks on a picture", () => {
    it("names the marked copy after the picture", () => {
        expect(derived_name("shot.png")).toBe("shot.marked.png");
        expect(derived_name("Screen Shot 2026.jpeg")).toBe("Screen Shot 2026.marked.png");
        expect(derived_name("noext")).toBe("noext.marked.png");
        expect(derived_name(".hidden")).toBe(".hidden.marked.png");
    });

    it("puts a box's corners in order however it was dragged", () => {
        expect(normalized_box([300, 90], [120, 40])).toEqual([
            [120, 40],
            [300, 90],
        ]);
    });

    it("knows where each mark's number goes", () => {
        const box: Mark = { kind: "box", points: [[300, 90], [120, 40]], text: "" };
        const arrow: Mark = { kind: "arrow", points: [[0, 0], [50, 60]], text: "" };
        const pin: Mark = { kind: "pin", points: [[7, 8]], text: "" };
        expect(badge_point(box)).toEqual([120, 40]);
        expect(badge_point(arrow)).toEqual([50, 60]);
        expect(badge_point(pin)).toEqual([7, 8]);
        expect(badge_point({ kind: "pen", points: [], text: "" })).toBe(null);
    });

    it("finds the mark whose number is under the pointer", () => {
        const box: Mark = { kind: "box", points: [[100, 100], [300, 200]], text: "" };
        const pin: Mark = { kind: "pin", points: [[500, 500]], text: "" };
        const circle = badge_circle(pin, 1)!;
        expect(circle.x).toBeGreaterThan(500);
        expect(circle.y).toBeLessThan(500);

        expect(badge_under([box, pin], 1, 102, 98)).toBe(0);
        expect(badge_under([box, pin], 1, circle.x + 3, circle.y - 3)).toBe(1);
        expect(badge_under([box, pin], 1, 250, 150)).toBe(null);
        expect(badge_under([box, pin], 0.5, 51, 49)).toBe(0);
    });

    it("keeps a drawn mark and drops a twitch", () => {
        expect(is_worth_keeping({ kind: "box", points: [[1, 1], [2, 2]], text: "" })).toBe(false);
        expect(is_worth_keeping({ kind: "box", points: [[1, 1], [40, 30]], text: "" })).toBe(true);
        expect(is_worth_keeping({ kind: "pen", points: [[1, 1], [2, 2]], text: "" })).toBe(false);
        expect(is_worth_keeping({ kind: "pen", points: [[1, 1], [2, 2], [3, 3]], text: "" })).toBe(true);
        expect(is_worth_keeping({ kind: "pin", points: [[1, 1]], text: "" })).toBe(true);
        expect(is_worth_keeping({ kind: "label", points: [], text: "x" })).toBe(false);
    });

    it("shows the picture and not its copy, and finds the copy from the picture", () => {
        expect(originals([picture, copy]).map((held) => held.name)).toEqual(["shot.png"]);
        expect(originals(undefined)).toEqual([]);
        expect(marked_copy_of([picture, copy], "shot.png")?.name).toBe("shot.marked.png");
        expect(marked_copy_of([picture], "shot.png")).toBe(undefined);
    });
});
