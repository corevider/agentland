import { describe, expect, it } from "vitest";

import { replacement_says } from "@/components/MemoryPanel";

describe("what a replacement did to the memory it replaces", () => {
    it("says what approving it would do, while it is still a proposal", () => {
        expect(replacement_says(false, true)).toBe("— approving this takes it out");
        expect(replacement_says(false, false)).toBe("— approving this takes it out");
    });

    it("says the old one is out only when the old one is actually out", () => {
        expect(replacement_says(true, false)).toBe("— already taken out of the brief");
    });

    it("says so when both sides of a correction are still in force", () => {
        expect(replacement_says(true, true)).toBe("— still being told to the crew");
    });
});
