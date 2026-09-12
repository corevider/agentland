import { describe, expect, it } from "vitest";

import { as_typed, quoted } from "@/lib/drops";

describe("paths dropped on a terminal", () => {
    it("leaves a plain path alone", () => {
        expect(quoted("/home/ege/data/drops/pane-1/1789-shot.png")).toBe("/home/ege/data/drops/pane-1/1789-shot.png");
    });

    it("quotes a path a shell would split", () => {
        expect(quoted("/tmp/Screenshot from today.png")).toBe("'/tmp/Screenshot from today.png'");
        expect(quoted("/tmp/a$b.txt")).toBe("'/tmp/a$b.txt'");
    });

    it("keeps a single quote inside a quoted path", () => {
        expect(quoted("/tmp/ege's notes.md")).toBe(`'/tmp/ege'\\''s notes.md'`);
    });

    it("hands several over at once, with room to keep typing", () => {
        expect(as_typed(["/tmp/a.png", "/tmp/b c.log"])).toBe("/tmp/a.png '/tmp/b c.log' ");
    });

    it("types nothing for nothing", () => {
        expect(as_typed([])).toBe("");
    });
});
