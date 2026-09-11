import { describe, expect, it } from "vitest";

import { checks_for } from "@/lib/checks";
import type { Agent, Entry, Task } from "@/lib/core";

const agent = (id: string, role: string, repository_id = "svc"): Agent =>
    ({ id, role, repository_id }) as unknown as Agent;

const reviewed = (by: string, verdict: string, summary = ""): Entry => ({
    what: { kind: "reviewed", verdict, summary },
    by,
    at: 0,
});

const card = (evidence: Entry[]): Task =>
    ({ id: "t1", repository_id: "svc", evidence }) as unknown as Task;

describe("the checks a card owes", () => {
    it("shows only the roles the crew holds on the card's project", () => {
        const crew = [agent("rex", "reviewer"), agent("sec", "security", "elsewhere"), agent("ada", "implementer")];

        expect(checks_for(card([]), crew).map((check) => check.role)).toEqual(["reviewer"]);
    });

    it("counts an approval as passed and says who gave it", () => {
        const checks = checks_for(card([reviewed("rex", "approved", "reads well")]), [agent("rex", "reviewer")]);

        expect(checks).toEqual([{ role: "reviewer", state: "passed", by: "rex", summary: "reads well", at: 0 }]);
    });

    it("does not let an approval stand once changes were asked for after it", () => {
        const crew = [agent("rex", "reviewer"), agent("sec", "security")];
        const said = [reviewed("rex", "approved"), reviewed("sec", "requested changes", "token in a log"), reviewed("sec", "approved")];

        const states = Object.fromEntries(checks_for(card(said), crew).map((check) => [check.role, check.state]));

        expect(states).toEqual({ reviewer: "waiting", security: "passed" });
    });

    it("says a check was sent back until that role reviews again", () => {
        const checks = checks_for(card([reviewed("tess", "requested changes", "no test for Bye")]), [agent("tess", "tester")]);

        expect(checks[0].state).toBe("changes");
        expect(checks[0].summary).toBe("no test for Bye");
    });

    it("treats a comment as not done", () => {
        const checks = checks_for(card([reviewed("rex", "commented")]), [agent("rex", "reviewer")]);

        expect(checks[0].state).toBe("waiting");
    });
});
