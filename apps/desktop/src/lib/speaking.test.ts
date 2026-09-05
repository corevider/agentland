import { beforeEach, describe, expect, it, vi } from "vitest";

const listen = vi.hoisted(() => vi.fn());
const can_listen = vi.hoisted(() => vi.fn());
const read_back = vi.hoisted(() => vi.fn());
const start_listening = vi.hoisted(() => vi.fn());
const stop_listening = vi.hoisted(() => vi.fn());

vi.mock("@/lib/listen", () => ({ can_listen, listen }));
vi.mock("@/lib/core", () => ({ read_back, start_listening, stop_listening }));

const { begin_speaking, end_speaking, listening_where } = await import("@/lib/speaking");

function a_recording(size: number) {
    return { stop: vi.fn(async () => ({ size, type: "audio/wav" }) as Blob) };
}

beforeEach(() => {
    vi.clearAllMocks();
    read_back.mockResolvedValue({ text: "widen the scope matrix" });
    stop_listening.mockResolvedValue({ text: "from the machine" });
    start_listening.mockResolvedValue(undefined);
});

describe("where the microphone is read from", () => {
    it("is this window when the window can do it, and nothing is spawned", async () => {
        can_listen.mockReturnValue(true);
        listen.mockResolvedValue(a_recording(4000));

        expect(await begin_speaking()).toBe("window");
        expect(start_listening).not.toHaveBeenCalled();
        expect(await end_speaking()).toBe("widen the scope matrix");
        expect(stop_listening).not.toHaveBeenCalled();
    });

    it("falls back to the machine when the window has no microphone", async () => {
        can_listen.mockReturnValue(false);

        expect(await begin_speaking()).toBe("machine");
        expect(await end_speaking()).toBe("from the machine");
    });

    it("falls back when the window is asked and refuses, rather than ending the feature", async () => {
        can_listen.mockReturnValue(true);
        listen.mockRejectedValue(new Error("NotAllowedError"));

        expect(await begin_speaking()).toBe("machine");
        expect(start_listening).toHaveBeenCalled();
    });

    it("does not ask a transcriber to read a press too short to hold a word", async () => {
        can_listen.mockReturnValue(true);
        listen.mockResolvedValue(a_recording(200));

        await begin_speaking();

        expect(await end_speaking()).toBe("");
        expect(read_back).not.toHaveBeenCalled();
    });

    it("is nowhere once the press is over, so the next one decides again", async () => {
        can_listen.mockReturnValue(true);
        listen.mockResolvedValue(a_recording(4000));

        await begin_speaking();
        expect(listening_where()).toBe("window");

        await end_speaking();
        expect(listening_where()).toBeNull();
    });
});
