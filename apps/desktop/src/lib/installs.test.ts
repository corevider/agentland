import { describe, expect, it } from "vitest";

import { recipe_for } from "@/lib/installs";

describe("how a missing tool is installed", () => {
    it("uses winget on Windows and the tool's own route elsewhere", () => {
        expect(recipe_for("npm", "windows")?.command).toBe("winget install OpenJS.NodeJS.LTS");
        expect(recipe_for("cargo", "linux")?.command).toContain("sh.rustup.rs");
        expect(recipe_for("uv", "macos")?.command).toContain("astral.sh/uv");
    });

    it("says what the tool is and where it lives", () => {
        const recipe = recipe_for("npm", "linux");
        expect(recipe?.what).toContain("Node.js");
        expect(recipe?.url).toBe("https://nodejs.org");
    });

    it("has nothing to offer for a tool or a platform it does not know", () => {
        expect(recipe_for("ghc", "linux")).toBeNull();
        expect(recipe_for("npm", "plan9")).toBeNull();
    });
});
