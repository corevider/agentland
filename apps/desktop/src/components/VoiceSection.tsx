import { useCallback, useEffect, useState } from "react";

import { Waiting } from "@/components/Spinner";
import { set_transcriber, voice_state, type VoiceState } from "@/lib/core";

/// Speaking to the crew.
///
/// Nothing is bundled: the recording is made by a program already on the
/// machine, and the words are read back by one named here. A microphone and a
/// speech model are both things to install on purpose, not to find installed.
export function VoiceSection() {
    const [state, set_state] = useState<VoiceState | null>(null);
    const [draft, set_draft] = useState("");
    const [notice, set_notice] = useState<string | null>(null);

    const refresh = useCallback(async () => {
        const held = await voice_state();
        set_state(held);
        set_draft((current) => current || held.transcriber || "");
    }, []);

    useEffect(() => {
        refresh().catch((cause) => set_notice(String(cause)));
    }, [refresh]);

    const save = useCallback(async () => {
        try {
            await set_transcriber(draft);
            set_notice(null);
            await refresh();
        } catch (cause) {
            set_notice(cause instanceof Error ? cause.message : String(cause));
        }
    }, [draft, refresh]);

    if (!state) {
        return <Waiting says="asking what this machine can hear…" className="font-mono text-[11px] text-shade" />;
    }

    return (
        <section className="flex flex-col gap-3">
            <p className="font-mono text-[11px] text-shade">
                {state.recorder
                    ? `Recording with ${state.recorder}, at 16 kHz in mono — what every speech model wants.`
                    : "No recorder on this machine. Install pw-record, parec or arecord and this comes back."}
            </p>

            <label className="flex flex-col gap-1">
                <span className="font-mono text-[10px] uppercase tracking-[0.14em] text-shade">
                    The command that reads a recording back
                </span>
                <input
                    className="rounded-lg border border-reef bg-lagoon px-2 py-1 font-mono text-[11px]"
                    placeholder="whisper-cli -m ~/models/ggml-base.en.bin -nt -f {file}"
                    value={draft}
                    onChange={(event) => set_draft(event.target.value)}
                    onKeyDown={(event) => {
                        if (event.key === "Enter") {
                            void save();
                        }
                    }}
                />
                <span className="font-mono text-[10px] text-shade">
                    {"{file}"} is where the recording goes; left out, the path is put on the end. It
                    runs on this machine and nothing is sent anywhere.
                </span>
                <span className="font-mono text-[10px] text-shade">
                    Left to guess, a model hears a short sentence as English. Name the language by
                    putting it in front of the command:{" "}
                    <span className="text-linen">AGENTLAND_WHISPER_LANGUAGE=tr</span> — or{" "}
                    <span className="text-linen">en</span>, or nothing at all to let it guess.
                </span>
            </label>

            <div className="flex items-center gap-2">
                <button
                    className="rounded-lg border border-turquoise px-2 py-1 font-mono text-[11px] text-turquoise"
                    onClick={() => void save()}
                >
                    save
                </button>
                <span className="font-mono text-[10px] text-shade">
                    {state.transcriber ? "set" : "not set — the button will say so"}
                </span>
            </div>

            <p className="font-mono text-[10px] text-shade">
                Hold “hold to speak” in the top bar. What you said is typed into the pane you are
                watching and left there: read it, then press enter yourself.
            </p>

            {notice ? <p className="font-mono text-[11px] text-coral">{notice}</p> : null}
        </section>
    );
}
