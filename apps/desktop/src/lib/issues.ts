import type { GitHubIssue, Repository } from "@/lib/core";

/// Whether a project's issues can be read: it needs a remote on GitHub.
export function on_github(repository: Repository): boolean {
    return repository.remotes.some((remote) => remote.provider === "github");
}

/// The line under an issue's title: who opened it and how it is labelled.
export function issue_line(issue: GitHubIssue): string {
    return [issue.author.login, ...issue.labels.map((label) => label.name)].filter(Boolean).join(" · ");
}

/// The list as it is once a card has been made from one of them.
export function with_card(issues: GitHubIssue[], number: number, card: string): GitHubIssue[] {
    return issues.map((issue) => (issue.number === number ? { ...issue, card } : issue));
}
