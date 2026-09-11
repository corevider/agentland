import type { Agent, Entry, Evidence, Task } from "@/lib/core";

/// The roles that judge somebody else's work, in the order a person reads them.
export const CHECKS = ["reviewer", "tester", "security"] as const;

export type CheckRole = (typeof CHECKS)[number];

/// Where one check stands: passed, sent back with changes asked for, or not
/// done yet.
export type CheckState = "passed" | "changes" | "waiting";

export interface Check {
    role: CheckRole;
    state: CheckState;
    by: string | null;
    summary: string;
    at: number;
}

const APPROVED = "approved";
const CHANGES = "requested changes";

interface Review {
    by: string;
    verdict: string;
    summary: string;
    at: number;
}

function reviews_of(task: Task): Review[] {
    return task.evidence
        .map((entry: Entry) => ({ entry, what: entry.what ?? (entry as unknown as Evidence) }))
        .filter(({ what }) => what.kind === "reviewed")
        .map(({ entry, what }) => ({
            by: entry.by,
            verdict: String(what.verdict ?? ""),
            summary: String(what.summary ?? ""),
            at: entry.at ?? 0,
        }));
}

/// Every check a card owes, and how each one stands.
///
/// The same rule the core merges by: a check is owed for each judging role the
/// crew holds on the card's project, and an approval only stands if nobody has
/// asked for changes since — a yes to code that has since moved is not a yes.
/// A role nobody was hired for is not shown, because it gates nothing.
export function checks_for(task: Task, agents: Agent[]): Check[] {
    const crew = agents.filter((agent) => agent.repository_id === task.repository_id);
    const role_of = (id: string) => crew.find((agent) => agent.id === id)?.role;
    const reviews = reviews_of(task);
    const sent_back = reviews.map((review) => review.verdict).lastIndexOf(CHANGES);
    const standing = reviews.slice(sent_back + 1).filter((review) => review.verdict === APPROVED);

    return CHECKS.filter((role) => crew.some((agent) => agent.role === role)).map((role) => {
        const passed = standing.filter((review) => role_of(review.by) === role).at(-1);
        if (passed) {
            return { role, state: "passed", by: passed.by, summary: passed.summary, at: passed.at };
        }

        const latest = reviews.filter((review) => role_of(review.by) === role).at(-1);
        return {
            role,
            state: latest?.verdict === CHANGES ? "changes" : "waiting",
            by: latest?.by ?? null,
            summary: latest?.summary ?? "",
            at: latest?.at ?? 0,
        };
    });
}
