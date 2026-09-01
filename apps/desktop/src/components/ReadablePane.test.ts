import { readFileSync } from "node:fs";
import { Terminal } from "@xterm/headless";
import { describe, expect, it } from "vitest";

import { lines_from_screen, readable_from_screen, unwrap, type Screen } from "@/components/ReadablePane";

/// The pane reads its own live emulator; here the same bytes go through the
/// same emulator so what the test reads is what the pane would show.
async function read(raw: string, cols = 160): Promise<string[]> {
    const terminal = new Terminal({ cols, rows: 48, scrollback: 4000, allowProposedApi: true });
    await new Promise<void>((done) => terminal.write(raw, () => done()));
    return unwrap(readable_from_screen(lines_from_screen(terminal as unknown as Screen)), cols);
}

describe("reading a pane instead of watching it", () => {
    it("reads a live agent's session as what it said", async () => {
        // Captured from a running agent through /sessions/{id}/log.
        const lines = await read(readFileSync("src/components/fixtures/live-pane.txt", "utf8"));
        const text = lines.join("\n");

        expect(text).toContain("I'll list the git worktrees and check each for");
        expect(text).toContain("Bash(git worktree list --porcelain)");
        expect(text).toContain("All 9 worktrees of agentland-svc-demo have clean");

        // The screen holds a spinner frame a second and a footer under every
        // turn; read line by line those letters land inside the sentences.
        expect(text).not.toMatch(/esc to interrupt/i);
        expect(text).not.toMatch(/bypass permissions/i);
        expect(text).not.toMatch(/^[✻✽✶✢✳]/m);
        expect(lines.length).toBeLessThan(200);
    });

    it("keeps the words apart when the engine moves the cursor instead of typing spaces", async () => {
        const lines = await read("\x1b[2G\x1b[1mAccessing\x1b[12Gworkspace:\x1b[22m");
        expect(lines).toEqual(["Accessing workspace:"]);
    });

    it("shows what a redraw left, not every frame of it", async () => {
        const lines = await read("Thinking…\rDone.\x1b[K");
        expect(lines).toEqual(["Done."]);
    });

    it("reads a captured first-run screen as sentences", async () => {
        const lines = await read(readFileSync("../../sessions/pane-6a8eac18-1.log", "utf8"));
        const text = lines.join("\n");

        expect(text).toContain("Accessing workspace:");
        expect(text).toContain("Quick safety check: Is this a project you created");
        expect(text).toContain("1. Yes, I trust this folder");
        expect(text).not.toContain("\x1b");
    });

    it("leaves out a rule the engine draws across the pane, whatever it carries", () => {
        expect(
            readable_from_screen([
                "⚠─1 MCP server needs authentication · run /mcp──────────────",
                "  Model: Fable 5 | Ctx: 35.2k",
                "● A readable pane shows what the reader came for.",
            ]),
        ).toEqual(["● A readable pane shows what the reader came for."]);
    });

    it("joins a paragraph the engine wrapped to the pane's width", () => {
        expect(
            unwrap(
                [
                    "The supervisor watches a step until it",
                    "settles, then wakes the commander.",
                    "",
                    "● Next: verify the delivery.",
                ],
                44,
            ),
        ).toEqual([
            "The supervisor watches a step until it settles, then wakes the commander.",
            "",
            "● Next: verify the delivery.",
        ]);
    });

    it("says a line the engine keeps restamping once", () => {
        expect(readable_from_screen(["working", "working", "working", "done"])).toEqual([
            "working",
            "done",
        ]);
    });

    it("keeps the shape of indented output while dropping the panel's own margin", () => {
        expect(
            readable_from_screen(["  function add() {", "      return 1;", "  }"]),
        ).toEqual(["function add() {", "    return 1;", "}"]);
    });
});
