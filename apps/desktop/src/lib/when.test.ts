import { describe, expect, it } from "vitest";

import { due_in, exactly, when } from "@/lib/when";

const now = 1_788_000_000;

describe("saying when something happened", () => {
    it("says nothing rather than lying about a record with no time", () => {
        expect(when(0, now)).toBe("no date");
        expect(exactly(0)).toBe("not recorded");
    });

    it("keeps the last hour in minutes", () => {
        expect(when(now - 30, now)).toBe("just now");
        expect(when(now - 60, now)).toBe("1m ago");
        expect(when(now - 45 * 60, now)).toBe("45m ago");
    });

    it("moves to hours, then days", () => {
        expect(when(now - 3 * 3600, now)).toBe("3h ago");
        expect(when(now - 2 * 86400, now)).toBe("2d ago");
    });

    it("becomes a date once a week has passed, because '412h ago' is not a thought", () => {
        expect(when(now - 30 * 86400, now)).not.toContain("ago");
    });

    it("does not say a clock skew happened in the future", () => {
        expect(when(now + 500, now)).toBe("just now");
    });
});

describe("saying when something is due", () => {
    it("counts forward", () => {
        expect(due_in(now + 90, now)).toBe("in 1m");
        expect(due_in(now + 4 * 3600, now)).toBe("in 4h");
        expect(due_in(now + 3 * 86400, now)).toBe("in 3d");
    });

    it("says due now once the time has passed", () => {
        expect(due_in(now - 10, now)).toBe("due now");
        expect(due_in(0, now)).toBe("not scheduled");
    });
});
