import { describe, expect, it } from "vitest";

import { anchor_of, notes_as_review, pinnable, read_patch } from "@/lib/annotations";

const PATCH = [
    "diff --git a/src/farewell.py b/src/farewell.py",
    "index d354ced..34de51a 100644",
    "--- a/src/farewell.py",
    "+++ b/src/farewell.py",
    "@@ -1,2 +1,3 @@",
    " def farewell(name):",
    '-    return "Bye, " + name',
    '+    return "Goodbye, " + name',
    "+",
    "@@ -10,2 +11,2 @@ def other():",
    "--- a comment that begins with two dashes",
    "+kept",
    "diff --git a/src/test_farewell.py b/src/test_farewell.py",
    "new file mode 100644",
    "--- /dev/null",
    "+++ b/src/test_farewell.py",
    "@@ -0,0 +1,1 @@",
    "+import unittest",
].join("\n");

describe("reading a patch", () => {
    const lines = read_patch(PATCH);
    const at = (text: string) => lines.find((line) => line.text === text)!;

    it("numbers context and added lines in the new file and removed lines in the old one", () => {
        expect(at(" def farewell(name):")).toMatchObject({ kind: "context", old_line: 1, new_line: 1 });
        expect(at('-    return "Bye, " + name')).toMatchObject({ kind: "removed", old_line: 2, new_line: null });
        expect(at('+    return "Goodbye, " + name')).toMatchObject({ kind: "added", new_line: 2 });
        expect(at("+")).toMatchObject({ kind: "added", new_line: 3 });
    });

    it("starts counting again at every hunk", () => {
        expect(at("+kept")).toMatchObject({ kind: "added", new_line: 11, file: "src/farewell.py" });
    });

    it("does not take a removed line starting with dashes for the next file", () => {
        expect(at("--- a comment that begins with two dashes")).toMatchObject({
            kind: "removed",
            old_line: 10,
            file: "src/farewell.py",
        });
    });

    it("follows the diff from file to file, new files included", () => {
        expect(at("+import unittest")).toMatchObject({ kind: "added", new_line: 1, file: "src/test_farewell.py" });
    });

    it("lets a note go only on code, never on a header", () => {
        expect(pinnable(at("diff --git a/src/farewell.py b/src/farewell.py"))).toBe(false);
        expect(pinnable(at("@@ -1,2 +1,3 @@"))).toBe(false);
        expect(pinnable(at('+    return "Goodbye, " + name'))).toBe(true);
    });

    it("points a note on a removed line at the old file", () => {
        expect(anchor_of(at('-    return "Bye, " + name'))).toEqual({
            file: "src/farewell.py",
            line: 2,
            side: "old",
            excerpt: 'return "Bye, " + name',
        });
    });
});

describe("notes as one request for changes", () => {
    it("orders the notes by file and line, and pins each to where it was left", () => {
        const said = notes_as_review([
            { file: "src/test_farewell.py", line: 1, side: "new", excerpt: "import unittest", text: "use pytest-style asserts" },
            { file: "src/farewell.py", line: 2, side: "old", excerpt: 'return "Bye, " + name', text: "keep a test\nfor this" },
        ]);

        expect(said.startsWith("2 notes on the diff")).toBe(true);
        expect(said.indexOf("src/farewell.py:2 (a removed line)")).toBeLessThan(said.indexOf("src/test_farewell.py:1"));
        expect(said).toContain("keep a test\n   for this");
    });

    it("says one note, not one notes", () => {
        expect(notes_as_review([{ file: "a.py", line: 1, side: "new", excerpt: "x", text: "y" }])).toMatch(/^1 note on/);
    });
});
