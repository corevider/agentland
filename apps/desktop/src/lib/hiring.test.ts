import { describe, expect, it } from "vitest";

import { hiring_targets, target_value, worktree_for } from "@/lib/hiring";
import type { Repository } from "@/lib/core";

function repo(id: string): Repository {
    return {
        id,
        name: id,
        primary_path: `/home/somebody/${id}`,
        default_branch: "main",
        remotes: [],
        origin: null,
    };
}

describe("hiring_targets", () => {
    it("offers a project a worktree of its own even when it already has some", () => {
        const targets = hiring_targets(
            [repo("citybidwars")],
            [{ repository_id: "citybidwars", name: "x" }],
        );

        expect(targets.map((target) => target_value(target))).toEqual([
            "citybidwars/",
            "citybidwars/x",
        ]);
    });

    it("offers a project with nothing in it somewhere to hire into", () => {
        const targets = hiring_targets([repo("citybidwars")], []);

        expect(targets).toHaveLength(1);
        expect(targets[0].worktree).toBe("");
        expect(targets[0].label).toContain("citybidwars");
    });

    it("keeps each project's worktrees under that project", () => {
        const targets = hiring_targets(
            [repo("citybidwars"), repo("atolye")],
            [
                { repository_id: "atolye", name: "desk" },
                { repository_id: "citybidwars", name: "x" },
            ],
        );

        expect(targets.map((target) => target_value(target))).toEqual([
            "citybidwars/",
            "citybidwars/x",
            "atolye/",
            "atolye/desk",
        ]);
    });

    it("offers nothing when no project is open", () => {
        expect(hiring_targets([], [])).toEqual([]);
    });
});

describe("worktree_for", () => {
    it("names the worktree after the agent who will own it", () => {
        expect(worktree_for("Ada")).toBe("ada");
        expect(worktree_for("Ada Lovelace")).toBe("ada-lovelace");
    });

    it("keeps a name a branch and a folder can both carry", () => {
        expect(worktree_for("  ada/lovelace  ")).toBe("ada-lovelace");
        expect(worktree_for("Önder")).toBe("nder");
    });

    it("steps past a worktree a dismissed agent left standing", () => {
        expect(worktree_for("x", ["x"])).toBe("x-2");
        expect(worktree_for("x", ["x", "x-2"])).toBe("x-3");
        expect(worktree_for("x", ["desk"])).toBe("x");
    });

    it("says a name with nothing in it is no name", () => {
        expect(worktree_for("!!!")).toBeNull();
        expect(worktree_for("   ")).toBeNull();
    });
});
