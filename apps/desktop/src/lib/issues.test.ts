import { describe, expect, it } from "vitest";

import type { GitHubIssue, Repository } from "@/lib/core";
import { issue_line, on_github, with_card } from "@/lib/issues";

const repository = (provider: string) =>
    ({
        id: "web",
        name: "web",
        primary_path: "/code/web",
        default_branch: "main",
        remotes: [{ name: "origin", url: "", host: null, owner: null, repo: null, provider }],
        origin: null,
    }) as unknown as Repository;

const issue = (number: number): GitHubIssue => ({
    number,
    title: "Cart total",
    body: "",
    url: `https://github.com/shop/web/issues/${number}`,
    labels: [{ name: "bug" }, { name: "cart" }],
    author: { login: "ada" },
    updatedAt: "",
    card: null,
});

describe("issues on the board", () => {
    it("are read only for a project with a remote on GitHub", () => {
        expect(on_github(repository("github"))).toBe(true);
        expect(on_github(repository("local"))).toBe(false);
    });

    it("say who opened them and how they are labelled", () => {
        expect(issue_line(issue(12))).toBe("ada · bug · cart");
    });

    it("show the card made from one without reading them again", () => {
        const after = with_card([issue(12), issue(13)], 13, "t40");

        expect(after.map((held) => held.card)).toEqual([null, "t40"]);
    });
});
