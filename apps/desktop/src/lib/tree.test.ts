import { describe, expect, it } from "vitest";

import {
    crumbs_of,
    hunks_of,
    is_probably_text,
    join_path,
    line_kind,
    parent_of,
    size_word,
    sort_entries,
} from "@/lib/tree";

describe("reading a project's folder", () => {
    it("puts folders above files, each alphabetically", () => {
        const sorted = sort_entries([
            { name: "server.js", kind: "file", size: 10 },
            { name: "test", kind: "dir", size: 0 },
            { name: "README.md", kind: "file", size: 20 },
            { name: "src", kind: "dir", size: 0 },
        ]);

        expect(sorted.map((entry) => entry.name)).toEqual(["src", "test", "README.md", "server.js"]);
    });

    it("walks up and down a path", () => {
        expect(join_path("", "src")).toBe("src");
        expect(join_path("src", "lib")).toBe("src/lib");
        expect(parent_of("src/lib/core.ts")).toBe("src/lib");
        expect(parent_of("README.md")).toBe("");
    });

    it("gives every crumb the path it walks to", () => {
        expect(crumbs_of("src/lib")).toEqual([
            { name: "root", path: "" },
            { name: "src", path: "src" },
            { name: "lib", path: "src/lib" },
        ]);
    });

    it("keeps binary files from being opened as text", () => {
        expect(is_probably_text("server.js")).toBe(true);
        expect(is_probably_text("Makefile")).toBe(true);
        expect(is_probably_text(".gitignore")).toBe(true);
        expect(is_probably_text("logo.PNG")).toBe(false);
    });

    it("says sizes the way a person reads them", () => {
        expect(size_word(512)).toBe("512 B");
        expect(size_word(2048)).toBe("2 KB");
        expect(size_word(3 * 1024 * 1024)).toBe("3.0 MB");
    });
});

describe("reading a patch", () => {
    const patch = [
        "diff --git a/server.js b/server.js",
        "index 111..222 100644",
        "--- a/server.js",
        "+++ b/server.js",
        "@@ -1,3 +1,6 @@",
        " const express = require('express')",
        "+app.get('/version', handler)",
        "-app.get('/old', handler)",
        "diff --git a/README.md b/README.md",
        "+## /version",
    ].join("\n");

    it("splits it by file", () => {
        const hunks = hunks_of(patch);

        expect(hunks.map((hunk) => hunk.file)).toEqual(["server.js", "README.md"]);
        expect(hunks[1].lines).toEqual(["+## /version"]);
    });

    it("has nothing to show for an empty patch", () => {
        expect(hunks_of("")).toEqual([]);
    });

    it("tells added from removed from noise", () => {
        expect(line_kind("+app.get")).toBe("added");
        expect(line_kind("-app.get")).toBe("removed");
        expect(line_kind("@@ -1,3 +1,6 @@")).toBe("meta");
        expect(line_kind("+++ b/server.js")).toBe("meta");
        expect(line_kind(" const express")).toBe("same");
    });
});
