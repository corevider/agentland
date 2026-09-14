import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { ask_for_file, folder_of, on_file_asked, take_asked_file } from "@/lib/asked_file";

const file = { repository_id: "shop", worktree: null, path: "src/app/cart.tsx" };

describe("a file asked for from outside the Files & Git panel", () => {
    beforeEach(() => {
        vi.stubGlobal("window", new EventTarget());
    });

    afterEach(() => {
        take_asked_file();
        vi.unstubAllGlobals();
    });

    it("waits for a panel that is not on screen yet, and is taken once", () => {
        ask_for_file(file);

        expect(take_asked_file()).toEqual(file);
        expect(take_asked_file()).toBeNull();
    });

    it("reaches a panel already on screen at once", () => {
        const heard: unknown[] = [];
        const stop = on_file_asked(() => heard.push(take_asked_file()));

        ask_for_file(file);
        stop();

        expect(heard).toEqual([file]);
    });

    it("lists the folder a file sits in", () => {
        expect(folder_of("src/app/cart.tsx")).toBe("src/app");
        expect(folder_of("README.md")).toBe("");
    });
});
