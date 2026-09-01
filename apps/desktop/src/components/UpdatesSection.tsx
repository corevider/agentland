import { useCallback, useEffect, useState } from "react";

import { Spinner, Waiting } from "@/components/Spinner";
import { is_tauri } from "@/lib/core";
import {
    can_check,
    can_install,
    in_a_sentence,
    notes_for_reading,
    type UpdateState,
} from "@/lib/updates";

const NOTE_LINES = 14;

/// What version this is, and whether there is a newer one.
///
/// The app checks on its own at startup; this is for the person who wants to ask
/// now rather than wait, and to read what changed before they take it.
export function UpdatesSection() {
    const [version, set_version] = useState<string | null>(null);
    const [state, set_state] = useState<UpdateState>({ kind: "idle" });
    const [now, set_now] = useState(() => Date.now());

    useEffect(() => {
        if (!is_tauri()) {
            set_state({
                kind: "off",
                why: "Updates are the desktop app's job — this is the interface in a browser.",
            });
            return;
        }

        import("@tauri-apps/api/app")
            .then((api) => api.getVersion())
            .then(set_version)
            .catch(() => undefined);
    }, []);

    // The "checked N minutes ago" line has to keep meaning what it says.
    useEffect(() => {
        const handle = window.setInterval(() => set_now(Date.now()), 30_000);
        return () => window.clearInterval(handle);
    }, []);

    const check = useCallback(async () => {
        set_state({ kind: "checking" });

        try {
            const { check: ask } = await import("@tauri-apps/plugin-updater");
            const found = await ask();

            if (!found) {
                set_state({ kind: "current", at: Date.now() });
                return;
            }

            set_state({
                kind: "available",
                version: found.version,
                notes: found.body ?? "",
                date: found.date ?? null,
            });
        } catch (cause) {
            set_state({
                kind: "trouble",
                why: cause instanceof Error ? cause.message : String(cause),
            });
        }
    }, []);

    const install = useCallback(async () => {
        if (state.kind !== "available") {
            return;
        }

        const wanted = state.version;
        set_state({ kind: "downloading", got: 0, total: null });

        try {
            const { check: ask } = await import("@tauri-apps/plugin-updater");
            const found = await ask();
            if (!found) {
                set_state({ kind: "current", at: Date.now() });
                return;
            }

            let got = 0;
            let total: number | null = null;

            await found.downloadAndInstall((event) => {
                if (event.event === "Started") {
                    total = event.data.contentLength ?? null;
                    set_state({ kind: "downloading", got: 0, total });
                } else if (event.event === "Progress") {
                    got += event.data.chunkLength;
                    set_state({ kind: "downloading", got, total });
                } else if (event.event === "Finished") {
                    set_state({ kind: "ready", version: wanted });
                }
            });

            set_state({ kind: "ready", version: wanted });
        } catch (cause) {
            set_state({
                kind: "trouble",
                why: cause instanceof Error ? cause.message : String(cause),
            });
        }
    }, [state]);

    const restart = useCallback(async () => {
        const { relaunch } = await import("@tauri-apps/plugin-process");
        await relaunch();
    }, []);

    const notes = state.kind === "available" ? notes_for_reading(state.notes, NOTE_LINES) : null;
    const busy = state.kind === "checking" || state.kind === "downloading";

    return (
        <div className="max-w-2xl">
            <p className="mb-4 max-w-prose text-sm text-driftwood">
                Every release is signed with a key only the build holds. A bundle that is not signed, or
                signed by anything else, is refused — which is what makes taking one safe.
            </p>

            <div className="flex flex-wrap items-baseline justify-between gap-4 border-b border-reef/60 py-3">
                <div>
                    <div className="text-sm text-linen">This is Agentland {version ?? "…"}</div>
                    <div className="flex items-center gap-1.5 font-mono text-[11px] text-shell">
                        {busy ? <Spinner /> : null}
                        {in_a_sentence(state, now)}
                    </div>
                </div>

                <div className="flex flex-wrap gap-2">
                    <button
                        className="rounded-lg border border-reef px-3 py-1 font-mono text-[11px] text-shell hover:border-foam disabled:opacity-40"
                        disabled={!can_check(state)}
                        onClick={() => void check()}
                    >
                        check now
                    </button>

                    {can_install(state) ? (
                        <button
                            className="rounded-lg border border-turquoise px-3 py-1 font-mono text-[11px] text-turquoise"
                            onClick={() => void install()}
                        >
                            download and install
                        </button>
                    ) : null}

                    {state.kind === "ready" ? (
                        <button
                            className="rounded-lg border border-palm px-3 py-1 font-mono text-[11px] text-palm"
                            onClick={() => void restart()}
                        >
                            restart now
                        </button>
                    ) : null}
                </div>
            </div>

            {state.kind === "downloading" ? (
                <div className="py-3">
                    <Waiting says="Taking it. Nothing is replaced until the download finishes." className="font-mono text-[11px] text-shade" />
                </div>
            ) : null}

            {notes && notes.text ? (
                <section className="py-3">
                    <h3 className="mb-1 font-mono text-[9px] uppercase tracking-[0.14em] text-shade">
                        What changed
                        {state.kind === "available" && state.date ? ` · ${state.date.slice(0, 10)}` : ""}
                    </h3>
                    <pre className="max-h-72 overflow-y-auto whitespace-pre-wrap rounded-lg border border-reef bg-lagoon px-3 py-2 font-mono text-[11px] leading-relaxed text-shell">
                        {notes.text}
                    </pre>
                    {notes.trimmed ? (
                        <p className="mt-1 font-mono text-[10px] text-shade">
                            The top of it. The rest is on the release page.
                        </p>
                    ) : null}
                </section>
            ) : null}
        </div>
    );
}
