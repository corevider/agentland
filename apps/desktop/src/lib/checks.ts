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
    head_sha: string;
    role: string;
    pull_url: string;
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
            head_sha: String(what.head_sha ?? ""),
            role: String(what.role ?? ""),
            pull_url: String(what.pull_url ?? ""),
        }));
}

/// Reviews apply only to the most recently observed pull-request commit.
/// The backend rechecks the forge before merging, including after a manual move.
export function checks_for(task: Task, _agents: Agent[]): Check[] {
    const head = task.evidence.filter((entry) => entry.what?.kind === "pull_observed").at(-1)?.what;
    const reviews = reviews_of(task).filter((review) =>
        !!head?.head_sha && review.head_sha === head.head_sha && review.pull_url === head.url && review.by !== task.assignee);
    const sent_back = reviews.map((review) => review.verdict).lastIndexOf(CHANGES);
    const standing = reviews.slice(sent_back + 1).filter((review) => review.verdict === APPROVED);
    return CHECKS.map((role) => {
        const passed = standing.filter((review) => review.role === role).at(-1);
        const proof = task.evidence.filter((entry) => entry.what?.kind === "tested" && entry.what.head_sha === head?.head_sha && entry.what.is_test === true && entry.by === passed?.by).at(-1);
        if (passed && (role !== "tester" || proof?.what.passed === true)) return { role, state: "passed", by: passed.by, summary: passed.summary, at: passed.at };
        const latest = reviews.filter((review) => review.role === role).at(-1);
        return { role, state: latest?.verdict === CHANGES ? "changes" : "waiting",
            by: latest?.by ?? null, summary: latest?.summary ?? "", at: latest?.at ?? 0 };
    });
}
