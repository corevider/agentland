import { describe, expect, it } from "vitest";

import {
    change_key,
    changes_in_flight,
    is_a_change,
    one_press_at_a_time,
    send_change,
    watch_changes,
} from "@/lib/pending";

function later<T>() {
    let resolve!: (value: T) => void;
    let reject!: (cause: unknown) => void;
    const promise = new Promise<T>((yes, no) => {
        resolve = yes;
        reject = no;
    });
    return { promise, resolve, reject };
}

describe("which calls are changes a person waits on", () => {
    it("counts what changes something, and not what only reads", () => {
        expect(is_a_change("/tasks")).toBe(false);
        expect(is_a_change("/tasks", { method: "POST" })).toBe(true);
        expect(is_a_change("/tasks/t1", { method: "DELETE" })).toBe(true);
        expect(is_a_change("/routines/r1", { method: "PATCH" })).toBe(true);
    });

    it("lets through the changes that come by the dozen and must each arrive", () => {
        expect(is_a_change("/sessions/p1/input", { method: "POST" })).toBe(false);
        expect(is_a_change("/sessions/p1/resize", { method: "POST" })).toBe(false);
        expect(is_a_change("/metrics", { method: "POST" })).toBe(false);
        expect(is_a_change("/tasks/t1/attachments/a.png/marks", { method: "PUT" })).toBe(false);
    });

    it("calls two changes the same only when method, path and body agree", () => {
        const body = JSON.stringify({ column: "done" });
        expect(change_key("/tasks/t1/move", { method: "POST", body })).toBe(
            change_key("/tasks/t1/move", { method: "POST", body }),
        );
        expect(change_key("/tasks/t1/move", { method: "POST", body })).not.toBe(
            change_key("/tasks/t1/move", { method: "POST", body: JSON.stringify({ column: "review" }) }),
        );
        expect(change_key("/tasks/t1/files", { method: "POST", body: new FormData() })).toBeNull();
        expect(change_key("/tasks")).toBeNull();
    });
});

describe("a change on its way", () => {
    it("answers the same change asked for again with the first, and sends it once", async () => {
        const answer = later<string>();
        let sent = 0;
        const send = () => {
            sent += 1;
            return answer.promise;
        };

        const first = send_change("POST /tasks/t1/move done", send);
        const second = send_change("POST /tasks/t1/move done", send);
        expect(second).toBe(first);
        expect(sent).toBe(1);

        answer.resolve("moved");
        await expect(second).resolves.toBe("moved");

        const again = send_change("POST /tasks/t1/move done", () => Promise.resolve("moved again"));
        await expect(again).resolves.toBe("moved again");
    });

    it("is counted while it runs, even when it fails, and says so as the count moves", async () => {
        const seen: number[] = [];
        const stop = watch_changes((count) => seen.push(count));
        const answer = later<void>();

        const running = send_change(null, () => answer.promise);
        expect(changes_in_flight()).toBe(1);

        answer.reject(new Error("refused"));
        await expect(running).rejects.toThrow("refused");
        expect(changes_in_flight()).toBe(0);
        expect(seen).toEqual([1, 0]);
        stop();
    });

    it("never folds changes whose bodies cannot be compared", async () => {
        let sent = 0;
        const send = () => {
            sent += 1;
            return Promise.resolve();
        };

        await Promise.all([send_change(null, send), send_change(null, send)]);
        expect(sent).toBe(2);
    });
});

describe("a control that is still working", () => {
    it("takes no second press until the first is done", async () => {
        const control = one_press_at_a_time();
        const answer = later<string>();

        const first = control.press(() => answer.promise);
        expect(control.busy()).toBe(true);
        expect(control.press(() => "second")).toBeNull();

        answer.resolve("done");
        await expect(first).resolves.toBe("done");
        expect(control.busy()).toBe(false);
        await expect(control.press(() => "next")).resolves.toBe("next");
    });

    it("takes presses again after one failed, including one that threw before it started", async () => {
        const control = one_press_at_a_time();

        await expect(
            control.press(() => {
                throw new Error("no core");
            }),
        ).rejects.toThrow("no core");
        expect(control.busy()).toBe(false);

        await expect(control.press(() => Promise.reject(new Error("refused")))).rejects.toThrow("refused");
        expect(control.busy()).toBe(false);
    });

    it("starts the work inside the press, while the click is still being handled", () => {
        const control = one_press_at_a_time();
        let started = false;

        control.press(() => {
            started = true;
        });
        expect(started).toBe(true);
    });
});
