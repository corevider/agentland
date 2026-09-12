import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { ask_for_card, on_card_asked, take_asked_card } from "@/lib/asked_card";

describe("a card asked for from outside the board", () => {
    beforeEach(() => {
        vi.stubGlobal("window", new EventTarget());
    });

    afterEach(() => {
        take_asked_card();
        vi.unstubAllGlobals();
    });

    it("waits for a board that is not on screen yet, and is taken once", () => {
        ask_for_card("t7");

        expect(take_asked_card()).toBe("t7");
        expect(take_asked_card()).toBeNull();
    });

    it("reaches a board already on screen at once", () => {
        const heard: (string | null)[] = [];
        const stop = on_card_asked(() => heard.push(take_asked_card()));

        ask_for_card("t8");
        stop();
        ask_for_card("t9");

        expect(heard).toEqual(["t8"]);
    });

    it("keeps only the latest ask", () => {
        ask_for_card("t1");
        ask_for_card("t2");

        expect(take_asked_card()).toBe("t2");
    });
});
