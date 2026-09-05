import { can_listen, listen, type Listening } from "@/lib/listen";
import { read_back, start_listening, stop_listening } from "@/lib/core";

/// Where the microphone is being read from for this press.
///
/// The window first, because it needs nothing installed and hands the core a
/// wav it does not have to convert. A machine recorder second, for a window
/// that has no microphone permission — refusing there should fall back rather
/// than end the feature.
type Where = "window" | "machine";

let held: Listening | null = null;
let where: Where | null = null;

export function listening_where(): Where | null {
    return where;
}

export async function begin_speaking(): Promise<Where> {
    if (can_listen()) {
        try {
            held = await listen();
            where = "window";
            return where;
        } catch {
            held = null;
        }
    }

    await start_listening();
    where = "machine";
    return where;
}

export async function end_speaking(): Promise<string> {
    const from = where;
    where = null;

    if (from === "window" && held) {
        const recording = held;
        held = null;

        const audio = await recording.stop();
        // A press too short to hold a word is silence, and asking a
        // transcriber to read silence is a wasted second and an error.
        if (audio.size <= 1000) {
            return "";
        }

        return (await read_back(audio)).text;
    }

    return (await stop_listening()).text;
}
