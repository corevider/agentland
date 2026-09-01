import { describe, expect, it } from "vitest";

import { as_url, clone_target, is_clonable } from "@/lib/pick";

describe("opening and cloning a project", () => {
    it("says where a clone will land", () => {
        expect(clone_target("https://github.com/anthropics/claude-code.git", "/home/dev/code")).toBe(
            "/home/dev/code/claude-code",
        );
        expect(clone_target("git@github.com:anthropics/claude-code.git", "/home/dev/code/")).toBe(
            "/home/dev/code/claude-code",
        );
    });

    it("says nothing when it cannot know", () => {
        expect(clone_target("", "/home/dev")).toBe(null);
        expect(clone_target("https://github.com/anthropics/claude-code", "")).toBe(null);
    });

    it("recognises what git can clone", () => {
        expect(is_clonable("https://github.com/anthropics/claude-code.git")).toBe(true);
        expect(is_clonable("git@github.com:anthropics/claude-code.git")).toBe(true);
        expect(is_clonable("anthropics/claude-code")).toBe(true);
        expect(is_clonable("just some words")).toBe(false);
        expect(is_clonable("/home/dev/code/thing")).toBe(false);
    });

    it("reads a bare owner/repo as GitHub, and leaves a real URL alone", () => {
        expect(as_url("anthropics/claude-code")).toBe("https://github.com/anthropics/claude-code.git");
        expect(as_url("https://gitlab.com/a/b.git")).toBe("https://gitlab.com/a/b.git");
    });
});
