import { describe, expect, it } from "vitest";

import { apply_order, move_onto, order_of, prune_order } from "@/lib/order";

const panes = [{ id: "a" }, { id: "b" }, { id: "c" }];

describe("the order terminals sit in", () => {
    it("follows the stored arrangement", () => {
        expect(order_of(apply_order(panes, ["c", "a", "b"]))).toEqual(["c", "a", "b"]);
    });

    it("leaves the core's order alone when nothing has been arranged", () => {
        expect(order_of(apply_order(panes, []))).toEqual(["a", "b", "c"]);
    });

    it("puts a terminal opened a moment ago at the end, not in the middle", () => {
        const fresh = [...panes, { id: "d" }];
        expect(order_of(apply_order(fresh, ["c", "a", "b"]))).toEqual(["c", "a", "b", "d"]);
    });

    it("ignores an arrangement that mentions terminals that have closed", () => {
        expect(order_of(apply_order(panes, ["gone", "b"]))).toEqual(["b", "a", "c"]);
    });
});

describe("moving one terminal onto another", () => {
    it("carries it down past the terminal it was dropped on", () => {
        expect(move_onto(["a", "b", "c"], "a", "c")).toEqual(["b", "c", "a"]);
        expect(move_onto(["a", "b", "c"], "a", "b")).toEqual(["b", "a", "c"]);
    });

    it("carries it up in front of the terminal it was dropped on", () => {
        expect(move_onto(["a", "b", "c"], "c", "a")).toEqual(["c", "a", "b"]);
        expect(move_onto(["a", "b", "c"], "c", "b")).toEqual(["a", "c", "b"]);
    });

    it("reaches both ends", () => {
        expect(move_onto(["a", "b", "c", "d"], "a", "d")).toEqual(["b", "c", "d", "a"]);
        expect(move_onto(["a", "b", "c", "d"], "d", "a")).toEqual(["d", "a", "b", "c"]);
    });

    it("does nothing when a terminal is dropped on itself", () => {
        expect(move_onto(["a", "b", "c"], "b", "b")).toEqual(["a", "b", "c"]);
    });

    it("does nothing when either terminal is not in the order", () => {
        expect(move_onto(["a", "b"], "a", "z")).toEqual(["a", "b"]);
        expect(move_onto(["a", "b"], "z", "a")).toEqual(["a", "b"]);
    });
});

describe("keeping the stored order tidy", () => {
    it("drops terminals that have closed", () => {
        expect(prune_order(["a", "b", "c"], ["a", "c"])).toEqual(["a", "c"]);
    });
});
