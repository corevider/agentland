import { describe, expect, it } from "vitest";

import { folder_name, standing_of } from "@/lib/shells";

const repos = [{ id: "svc", primary_path: "/home/ege/code/svc" }];
const worktrees = [
    { repository_id: "svc", name: "ada-tree", path: "/home/ege/code/svc/.wt/ada-tree" },
    { repository_id: "svc", name: "x-desk", path: "/data/worktrees/svc/x-desk" },
];

describe("where a pane stands", () => {
    it("names the worktree a folder is inside", () => {
        expect(standing_of("/data/worktrees/svc/x-desk/src", repos, worktrees)?.worktree).toBe("x-desk");
    });

    it("prefers a worktree nested under the checkout over the checkout", () => {
        expect(standing_of("/home/ege/code/svc/.wt/ada-tree", repos, worktrees)?.worktree).toBe("ada-tree");
        expect(standing_of("/home/ege/code/svc/src", repos, worktrees)?.worktree).toBeNull();
    });

    it("does not mistake a sibling folder with the same prefix", () => {
        expect(standing_of("/home/ege/code/svc-other", repos, worktrees)).toBeNull();
        expect(standing_of(null, repos, worktrees)).toBeNull();
    });

    it("names a folder by its last part", () => {
        expect(folder_name("/data/worktrees/svc/x-desk/")).toBe("x-desk");
    });
});
