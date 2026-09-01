import { describe, expect, it } from "vitest";

import { name_trouble } from "@/lib/naming";

describe("naming a project that is about to be scaffolded", () => {
    it("takes the names people actually use", () => {
        for (const name of ["svc-demo", "web", "app_2", "next.thing", "3d"]) {
            expect(name_trouble(name), name).toBeNull();
        }
    });

    it("refuses a name that would be read as a flag", () => {
        expect(name_trouble("-rf")).not.toBeNull();
        expect(name_trouble("--force")).not.toBeNull();
    });

    it("refuses a name that would write outside the folder", () => {
        expect(name_trouble("../elsewhere")).not.toBeNull();
        expect(name_trouble("a/b")).not.toBeNull();
        expect(name_trouble("..")).not.toBeNull();
    });

    it("refuses a name that would become two arguments", () => {
        expect(name_trouble("my app")).not.toBeNull();
        expect(name_trouble("app;rm")).not.toBeNull();
        expect(name_trouble("app$(id)")).not.toBeNull();
    });

    it("says what is wrong rather than only that something is", () => {
        expect(name_trouble("")).toContain("needs a name");
        expect(name_trouble("Web")).toContain("lowercase");
    });
});
