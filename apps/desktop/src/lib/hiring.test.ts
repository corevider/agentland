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
    it("offers every worktree there is", () => {
        const targets = hiring_targets(
            [repo("citybidwars")],
            [{ repository_id: "citybidwars", name: "lobby" }],
        );

        expect(targets).toHaveLength(1);
        expect(targets[0].worktree).toBe("lobby");
        expect(target_value(targets[0])).toBe("citybidwars/lobby");
    });

    it("offers a project that has no worktree yet, rather than nothing at all", () => {
        const targets = hiring_targets([repo("citybidwars")], []);

        expect(targets).toHaveLength(1);
        expect(targets[0].repository_id).toBe("citybidwars");
        expect(targets[0].worktree).toBe("");
        expect(targets[0].label).toContain("citybidwars");
    });

    it("does not offer to cut a second first worktree for a project that has one", () => {
        const targets = hiring_targets(
            [repo("citybidwars"), repo("atolye")],
            [{ repository_id: "citybidwars", name: "lobby" }],
        );

        expect(targets.map((target) => target_value(target))).toEqual([
            "citybidwars/lobby",
            "atolye/",
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

    it("says a name with nothing in it is no name", () => {
        expect(worktree_for("!!!")).toBeNull();
        expect(worktree_for("   ")).toBeNull();
    });
});
