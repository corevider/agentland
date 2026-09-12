import { describe, expect, it } from "vitest";

import type { Notice, NoticeReport } from "@/lib/core";
import { with_marked } from "@/lib/notices";

const notice = (id: number, kind: Notice["kind"], seen: boolean): Notice => ({
    id,
    kind,
    text: `notice ${id}`,
    workspace_id: null,
    repository_id: "shop",
    agent_id: "ada",
    opens: "agent:ada",
    at: 0,
    seen,
});

const report: NoticeReport = {
    notices: [notice(3, "waiting", false), notice(2, "finished", false), notice(1, "word", true)],
    unseen: 2,
    loud: true,
    desktop: true,
};

describe("the bell before the core answers", () => {
    it("marks one read and counts it off", () => {
        const marked = with_marked(report, [2], true);

        expect(marked.notices.map((held) => held.seen)).toEqual([false, true, true]);
        expect(marked.unseen).toBe(1);
        expect(marked.loud).toBe(true);
    });

    it("stops being loud once the one waiting on a person is read", () => {
        const marked = with_marked(report, [3], true);

        expect(marked.unseen).toBe(1);
        expect(marked.loud).toBe(false);
    });

    it("puts a read one back to count again", () => {
        const marked = with_marked(report, [1], false);

        expect(marked.notices[2].seen).toBe(false);
        expect(marked.unseen).toBe(3);
    });

    it("turns loud again when a waiting notice is put back", () => {
        const quiet = with_marked(report, [3], true);

        expect(with_marked(quiet, [3], false).loud).toBe(true);
    });

    it("reads every one when none is named", () => {
        const marked = with_marked(report, [], true);

        expect(marked.notices.every((held) => held.seen)).toBe(true);
        expect(marked.unseen).toBe(0);
        expect(marked.loud).toBe(false);
    });

    it("un-reads none when none is named", () => {
        expect(with_marked(report, [], false)).toEqual(report);
    });

    it("does not count a notice twice that was already where it is being put", () => {
        expect(with_marked(report, [1], true).unseen).toBe(2);
    });
});
