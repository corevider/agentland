import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { read_machine } from "@/lib/core";
import { wait_for_core } from "@/lib/startup";

vi.mock("@/lib/core", () => ({ read_machine: vi.fn() }));
const probe = vi.mocked(read_machine);
const machine = { os: "windows", shell: "powershell" };

beforeEach(() => { vi.useFakeTimers(); probe.mockReset(); });
afterEach(() => { vi.useRealTimers(); });

describe("cold-start connection", () => {
    it("waits through connection refusals and opens once the service answers", async () => {
        probe.mockRejectedValueOnce(new TypeError("Failed to fetch"))
            .mockRejectedValueOnce(new TypeError("Failed to fetch"))
            .mockResolvedValue(machine);
        const ready = wait_for_core(new AbortController().signal);
        await vi.advanceTimersByTimeAsync(500);
        await expect(ready).resolves.toBeUndefined();
        expect(probe).toHaveBeenCalledTimes(3);
        expect(vi.getTimerCount()).toBe(0);
    });

    it("does not delay an already running service", async () => {
        probe.mockResolvedValue(machine);
        await wait_for_core(new AbortController().signal);
        expect(probe).toHaveBeenCalledTimes(1);
        expect(vi.getTimerCount()).toBe(0);
    });

    it("bounds a hung HTTP probe and retries it", async () => {
        probe.mockImplementationOnce((signal) => new Promise((_, reject) => {
            signal!.addEventListener("abort", () => reject(new DOMException("Aborted", "AbortError")), { once: true });
        })).mockResolvedValue(machine);
        const ready = wait_for_core(new AbortController().signal);
        await vi.advanceTimersByTimeAsync(2250);
        await ready;
        expect(probe).toHaveBeenCalledTimes(2);
    });

    it("reports an unavailable service and allows a fresh attempt", async () => {
        probe.mockRejectedValue(new TypeError("Failed to fetch"));
        const failed = expect(wait_for_core(new AbortController().signal)).rejects.toThrow("Try again");
        await vi.advanceTimersByTimeAsync(30_000);
        await failed;
        expect(vi.getTimerCount()).toBe(0);
        probe.mockResolvedValue(machine);
        await wait_for_core(new AbortController().signal);
    });

    it("cancels a pending retry on unmount without further probes", async () => {
        probe.mockRejectedValue(new TypeError("Failed to fetch"));
        const controller = new AbortController();
        const cancelled = expect(wait_for_core(controller.signal)).rejects.toThrow();
        await vi.advanceTimersByTimeAsync(100);
        controller.abort();
        await cancelled;
        await vi.advanceTimersByTimeAsync(1000);
        expect(probe).toHaveBeenCalledTimes(1);
        expect(vi.getTimerCount()).toBe(0);
    });
});
