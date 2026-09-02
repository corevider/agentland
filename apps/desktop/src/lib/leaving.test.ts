import { describe, expect, it } from "vitest";

import { what_is_held } from "@/lib/leaving";
import type { Holdings } from "@/lib/core";

function holding(over: Partial<Holdings> = {}): Holdings {
    return {
        cards: [],
        pane_running: false,
        uncommitted: 0,
        unpushed: 0,
        empty_handed: true,
        ...over,
    };
}

describe("what_is_held", () => {
    it("says nothing about an agent with nothing in hand", () => {
        expect(what_is_held(holding())).toEqual([]);
    });

    it("names each card rather than counting them, and says where it goes", () => {
        const lines = what_is_held(
            holding({ cards: [{ id: "t2", title: "test", column: "working" }], empty_handed: false }),
        );

        expect(lines).toHaveLength(1);
        expect(lines[0]).toContain("t2 · test");
        expect(lines[0]).toContain("goes back to the board");
    });

    it("counts work that only exists in that worktree", () => {
        const lines = what_is_held(holding({ uncommitted: 1, unpushed: 3, empty_handed: false }));

        expect(lines[0]).toBe("1 file changed and never committed");
        expect(lines[1]).toBe("3 commits on its branch and nowhere else");
    });

    it("mentions the open pane, which closing is part of letting somebody go", () => {
        expect(what_is_held(holding({ pane_running: true, empty_handed: false }))).toEqual([
            "its pane is open and will be closed",
        ]);
    });
});
