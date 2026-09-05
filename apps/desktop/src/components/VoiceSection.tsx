import { useCallback, useEffect, useState } from "react";

import { Waiting } from "@/components/Spinner";
import { fetch_whisper, set_transcriber, voice_state, type VoiceState } from "@/lib/core";
import { can_listen } from "@/lib/listen";

/// Speaking to the crew.
///
/// The recording is made by this window itself, so nothing has to be installed
/// for it. Reading the words back needs a model, and that is a download rather
/// than an install: whisper.cpp and its weights are fetched here, kept in the
/// data folder, and never sent anywhere.
export function VoiceSection() {
    const [state, set_state] = useState<VoiceState | null>(null);
    // Null means "whatever the core says": the line is written for a person
    // when whisper is fetched, and an input that kept its first value would go
    // on showing the empty one it was born with.
    const [draft, set_draft] = useState<string | null>(null);
    const [notice, set_notice] = useState<string | null>(null);

    const refresh = useCallback(async () => {
        set_state(await voice_state());
    }, []);

    useEffect(() => {
        refresh().catch((cause) => set_notice(String(cause)));
    }, [refresh]);

    // While something is being fetched, ask often enough to show which half.
    useEffect(() => {
        if (!state?.whisper.fetching) {
            return;
        }

        const handle = window.setInterval(() => {
            refresh().catch(() => undefined);
        }, 1500);
        return () => window.clearInterval(handle);
    }, [state?.whisper.fetching, refresh]);

    const get_whisper = useCallback(
        async (model: string) => {
            try {
                set_notice(null);
                await fetch_whisper(model);
                set_draft(null);
                await refresh();
            } catch (cause) {
                set_notice(cause instanceof Error ? cause.message : String(cause));
            }
        },
        [refresh],
    );

    const save = useCallback(
        async (line: string) => {
            try {
                await set_transcriber(line);
                set_notice(null);
                set_draft(null);
                await refresh();
            } catch (cause) {
                set_notice(cause instanceof Error ? cause.message : String(cause));
            }
        },
        [refresh],
    );

    if (!state) {
        return <Waiting says="asking what this machine can hear…" className="font-mono text-[11px] text-shade" />;
    }

    return (
        <section className="flex flex-col gap-3">
            <p className="font-mono text-[11px] text-shade">
                {can_listen()
                    ? "Recorded by this window, at 16 kHz in mono — what every speech model wants, so nothing converts it."
                    : state.recorder
                      ? `Recorded with ${state.recorder}, at 16 kHz in mono.`
                      : "Nothing here can record."}
            </p>

            <div className="flex flex-col gap-2 rounded-lg border border-reef bg-lagoon-deep p-2">
                <span className="font-mono text-[10px] uppercase tracking-[0.14em] text-shade">
                    Whisper, on this machine
                </span>

                {state.whisper.fetching ? (
                    <Waiting
                        says={`${state.whisper.fetching}…`}
                        className="font-mono text-[11px] text-turquoise"
                    />
                ) : state.whisper.ready ? (
                    <p className="font-mono text-[11px] text-palm">
                        Here, and reading everything back on this machine. Nothing is sent anywhere.
                    </p>
                ) : !state.whisper.build ? (
                    <p className="font-mono text-[11px] text-sun">
                        whisper.cpp publishes no build for this platform. Install whisper-cli
                        yourself and name it below.
                    </p>
                ) : (
                    <>
                        <p className="font-mono text-[11px] text-shade">
                            Pick one and it is fetched — the program and its weights — into the data
                            folder. It is a download, once, not an install.
                        </p>
                        <div className="flex flex-wrap gap-2">
                            {state.whisper.models.map((model) => (
                                <button
                                    key={model.id}
                                    className="flex flex-col items-start gap-0.5 rounded-lg border border-reef px-2 py-1 text-left font-mono text-[11px] text-shell hover:border-turquoise hover:text-turquoise"
                                    onClick={() => void get_whisper(model.id)}
                                    title={model.says}
                                >
                                    <span>
                                        {model.id}
                                        {model.id === state.whisper.by_default ? " · recommended" : ""}
                                    </span>
                                    <span className="text-[10px] text-shade">
                                        {model.megabytes} MB — {model.says}
                                    </span>
                                </button>
                            ))}
                        </div>
                    </>
                )}
            </div>

            <label className="flex flex-col gap-1">
                <span className="font-mono text-[10px] uppercase tracking-[0.14em] text-shade">
                    The command that reads a recording back — written for you above, or your own
                </span>
                <input
                    className="rounded-lg border border-reef bg-lagoon px-2 py-1 font-mono text-[11px]"
                    placeholder="whisper-cli -m ~/models/ggml-base.en.bin -nt -f {file}"
                    value={draft ?? state.transcriber ?? ""}
                    onChange={(event) => set_draft(event.target.value)}
                    onKeyDown={(event) => {
                        if (event.key === "Enter") {
                            void save(event.currentTarget.value);
                        }
                    }}
                />
                <span className="font-mono text-[10px] text-shade">
                    {"{file}"} is where the recording goes; left out, the path is put on the end. It
                    runs on this machine and nothing is sent anywhere.
                </span>
                <span className="font-mono text-[10px] text-shade">
                    The line written above asks for <span className="text-linen">-l auto</span>, so
                    it hears whichever language it is spoken to in. Say{" "}
                    <span className="text-linen">-l tr</span> instead if you only ever dictate in
                    one: a short sentence is easier to place when it does not have to be guessed.
                </span>
            </label>

            <div className="flex items-center gap-2">
                <button
                    className="rounded-lg border border-turquoise px-2 py-1 font-mono text-[11px] text-turquoise"
                    onClick={() => void save(draft ?? state.transcriber ?? "")}
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
