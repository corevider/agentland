import { describe, expect, it } from "vitest";

import { folder_name, names_the_agent, place_label, places_in, settled_place, standing_of } from "@/lib/shells";

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

const known = {
    repos: [
        { id: "svc", primary_path: "/home/ege/code/svc", default_branch: "main" },
        { id: "gone", primary_path: "/home/ege/code/gone", default_branch: "main", missing: true },
    ],
    trees: [
        { repository_id: "svc", name: "x-desk", path: "/data/worktrees/svc/x-desk", branch: "agent/x-desk" },
        { repository_id: "svc", name: "old", path: "/data/worktrees/svc/old", branch: "agent/old", missing: true },
        { repository_id: "other", name: "theirs", path: "/data/worktrees/other/theirs", branch: "agent/theirs" },
    ],
};

describe("where a CLI can open", () => {
    it("offers the main checkout first, then the project's own worktrees", () => {
        expect(places_in(known, "svc").map((place) => place.path)).toEqual([
            "/home/ege/code/svc",
            "/data/worktrees/svc/x-desk",
        ]);
    });

    it("names a place by what it is, not by its path", () => {
        expect(places_in(known, "svc")[0].label).toBe("main checkout · main");
        expect(places_in(known, "svc")[1].label).toBe("x-desk · agent/x-desk");
    });

    it("leaves out what is gone from disk, checkout and worktree alike", () => {
        expect(places_in(known, "gone")).toEqual([]);
        expect(places_in(known, "svc").some((place) => place.path.endsWith("/old"))).toBe(false);
    });

    it("offers nothing for a project it has never heard of", () => {
        expect(places_in(known, "nobody")).toEqual([]);
    });
});

describe("the place a form is holding", () => {
    it("keeps a place that is on offer", () => {
        expect(settled_place("/data/worktrees/svc/x-desk", places_in(known, "svc"))).toBe(
            "/data/worktrees/svc/x-desk",
        );
    });

    it("replaces a place that is not, so the form cannot read as one and hold another", () => {
        expect(settled_place("/home/ege/code/deleted", places_in(known, "svc"))).toBe("/home/ege/code/svc");
    });

    it("leaves the value alone when there is nothing to offer instead", () => {
        expect(settled_place("/home/ege/code/gone", places_in(known, "gone"))).toBe("/home/ege/code/gone");
    });
});

describe("what a footer calls the folder a pane is in", () => {
    it("names a worktree with the project it was cut from", () => {
        expect(place_label("/data/worktrees/svc/x-desk/src", repos, worktrees)).toBe("svc/x-desk");
    });

    it("names a main checkout by the project alone", () => {
        expect(place_label("/home/ege/code/svc/src", repos, worktrees)).toBe("svc");
    });

    it("falls back to the folder's own name outside every project", () => {
        expect(place_label("/tmp/somewhere/else", repos, worktrees)).toBe("else");
    });

    it("says nothing when the pane stands nowhere it knows of", () => {
        expect(place_label(null, repos, worktrees)).toBeNull();
    });
});

describe("whether a pane already says who is in it", () => {
    it("counts a name the pane's own title spells out", () => {
        expect(names_the_agent("X · commander", "X")).toBe(true);
        expect(names_the_agent("Vega", "Vega")).toBe(true);
    });

    it("does not count a longer word that merely starts with it", () => {
        expect(names_the_agent("Xavier's branch", "X")).toBe(false);
        expect(names_the_agent("Vegan menu", "Vega")).toBe(false);
    });

    it("says no for a name the pane does not mention", () => {
        expect(names_the_agent("auth refactor", "Vega")).toBe(false);
    });

    it("is not thrown by a name with regex punctuation in it", () => {
        expect(names_the_agent("c++ work", "c++")).toBe(true);
        expect(names_the_agent("auth refactor", "c++")).toBe(false);
    });

    it("says no when there is nothing to compare", () => {
        expect(names_the_agent(null, "Vega")).toBe(false);
        expect(names_the_agent("auth refactor", null)).toBe(false);
    });
});
