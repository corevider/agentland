import { describe, expect, it } from "vitest";

import { checks_for } from "@/lib/checks";
import type { Agent, Entry, Task } from "@/lib/core";

const agent = (id: string, role: string, repository_id = "svc"): Agent =>
    ({ id, role, repository_id }) as unknown as Agent;

const reviewed = (by: string, verdict: string, summary = ""): Entry => ({
    what: { kind: "reviewed", verdict, summary, head_sha: "abc", pull_url: "pull/1", role: ({ rex: "reviewer", tess: "tester", sec: "security" } as Record<string, string>)[by] },
    by,
    at: 0,
});

const card = (evidence: Entry[]): Task =>
    ({ id: "t1", repository_id: "svc", evidence: [{ what: { kind: "pull_observed", head_sha: "abc", url: "pull/1" }, by: "forge", at: 0 }, ...evidence] }) as unknown as Task;

describe("the checks a card owes", () => {
    it("requires all check roles even when the crew has not hired them", () => {
        const crew = [agent("rex", "reviewer"), agent("sec", "security", "elsewhere"), agent("ada", "implementer")];

        expect(checks_for(card([]), crew).map((check) => check.role)).toEqual(["reviewer", "tester", "security"]);
    });

    it("counts an approval as passed and says who gave it", () => {
        const checks = checks_for(card([reviewed("rex", "approved", "reads well")]), [agent("rex", "reviewer")]);

        expect(checks.filter((check) => check.role === "reviewer")).toEqual([{ role: "reviewer", state: "passed", by: "rex", summary: "reads well", at: 0 }]);
    });

    it("does not let an approval stand once changes were asked for after it", () => {
        const crew = [agent("rex", "reviewer"), agent("sec", "security")];
        const said = [reviewed("rex", "approved"), reviewed("sec", "requested changes", "token in a log"), reviewed("sec", "approved")];

        const states = Object.fromEntries(checks_for(card(said), crew).map((check) => [check.role, check.state]));

        expect(states).toEqual({ reviewer: "waiting", tester: "waiting", security: "passed" });
    });

    it("says a check was sent back until that role reviews again", () => {
        const checks = checks_for(card([reviewed("tess", "requested changes", "no test for Bye")]), [agent("tess", "tester")]);

        expect(checks[1].state).toBe("changes");
        expect(checks[1].summary).toBe("no test for Bye");
    });

    it("treats a comment as not done", () => {
        const checks = checks_for(card([reviewed("rex", "commented")]), [agent("rex", "reviewer")]);

        expect(checks[0].state).toBe("waiting");
    });
});

it("invalidates approvals when the forge observes a new commit", () => {
    const task = card([reviewed("rex", "approved")]);
    task.evidence.push({ what: { kind: "pull_observed", head_sha: "def", url: "pull/1" }, by: "forge", at: 2 });
    expect(checks_for(task, [agent("rex", "reviewer")])[0].state).toBe("waiting");
});

it("does not count legacy reviews without a commit", () => {
    const review = reviewed("rex", "approved");
    delete review.what.head_sha;
    expect(checks_for(card([review]), [agent("rex", "reviewer")])[0].state).toBe("waiting");
});

it("requires a passing current-commit test proof for a tester approval", () => {
    const task = card([reviewed("tess", "approved")]);
    expect(checks_for(task, [agent("tess", "tester")])[1].state).toBe("waiting");
    task.evidence.push({ what: { kind: "tested", head_sha: "abc", is_test: true, passed: true }, by: "tess", at: 1 });
    expect(checks_for(task, [agent("tess", "tester")])[1].state).toBe("passed");
    task.evidence.push({ what: { kind: "tested", head_sha: "abc", is_test: true, passed: false }, by: "tess", at: 2 });
    expect(checks_for(task, [agent("tess", "tester")])[1].state).toBe("waiting");
});
