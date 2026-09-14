import { describe, expect, it } from "vitest";

import type { Agent, Service } from "@/lib/core";
import { design_note, page_on, path_of, recipients, worth_saying, type Pick } from "@/lib/design";

const service: Service = {
    key: "shop/ada-tree",
    repository_id: "shop",
    worktree: "ada-tree",
    port: 4101,
    session_id: "pane-9",
    state: "ready",
    command: "npm run dev",
    detected_from: "package.json",
    url: "http://127.0.0.1:4101",
};

const agent = (id: string, worktree: string, repository_id = "shop") =>
    ({ id, name: id, worktree, repository_id }) as Agent;

const pick: Pick = {
    url: "http://127.0.0.1:40331/cart?step=2#pay",
    selector: "main > button.pay:nth-of-type(2)",
    tag: "button",
    html: '<button class="pay">Pay  now</button>',
    text: "Pay\n  now",
    styles: { display: "inline-flex", color: "rgb(255, 255, 255)", margin: "0px", "z-index": "auto", border: "0px none rgb(0, 0, 0)" },
    box: { x: 24, y: 310, width: 120, height: 36 },
    viewport: { width: 1280, height: 800 },
};

describe("a pick from the preview", () => {
    it("goes first to whoever works where the dev server runs", () => {
        const crew = [agent("rex", "rex-tree"), agent("ada", "ada-tree"), agent("iris", "iris-tree", "blog")];

        expect(recipients(service, crew).map((held) => held.id)).toEqual(["ada", "rex"]);
    });

    it("names the page by the dev server's own address, not the preview's", () => {
        expect(page_on(service, pick.url)).toBe("http://127.0.0.1:4101/cart?step=2#pay");
        expect(page_on(service, "not a url")).toBe("http://127.0.0.1:4101");
    });

    it("tells only the styles the page set", () => {
        expect(worth_saying("inline-flex")).toBe(true);
        expect(worth_saying("0px")).toBe(false);
        expect(worth_saying("0px none rgb(0, 0, 0)")).toBe(false);
    });

    it("puts the person's words first and enough after them to find the element", () => {
        const note = design_note(pick, "  Make this green and a little bigger ", service);
        const lines = note.split("\n");

        expect(lines[0]).toBe("Make this green and a little bigger");
        expect(note).toContain("on http://127.0.0.1:4101/cart?step=2#pay");
        expect(note).toContain("- element: main > button.pay:nth-of-type(2) (button, 120×36 at 24,310 in a 1280×800 view)");
        expect(note).toContain('- its text: "Pay now"');
        expect(note).toContain('```html\n<button class="pay">Pay  now</button>\n```');
        expect(note).toContain("  display: inline-flex;");
        expect(note).not.toContain("margin");
        expect(note).not.toContain("40331");
        expect(note).not.toContain("a picture of it");
    });

    it("asks the dev server for the page's path and query, not its fragment", () => {
        expect(path_of(pick.url)).toBe("/cart?step=2");
        expect(path_of("not a url")).toBe("/");
    });

    it("hands over the picture when one was taken, and says what it is", () => {
        const note = design_note(pick, "Bigger", service, "/data/drops/shots/1789-element.png");

        expect(note).toContain(
            "- a picture of it, cut from the page rendered again at the same width (a menu held open or text typed in is not in it): /data/drops/shots/1789-element.png",
        );
        expect(note.trim().endsWith("say what you changed.")).toBe(true);
    });
});
