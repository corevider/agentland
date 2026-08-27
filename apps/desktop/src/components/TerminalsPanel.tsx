import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import { TerminalPane } from "@/components/TerminalPane";
import { is_tauri, list_windows, set_window } from "@/lib/core";
import { use_services } from "@/workspace/registry";

export function TerminalsPanel({ active }: { active: boolean }) {
    const services = use_services();
    const [zoomed, set_zoomed] = useState<string | null>(null);
    const [elsewhere, set_elsewhere] = useState<Record<string, string>>({});

    useEffect(() => {
        if (!active) {
            return;
        }

        const read = () => list_windows().then(set_elsewhere).catch(() => undefined);
        read();
        const handle = window.setInterval(read, 3000);
        return () => window.clearInterval(handle);
    }, [active]);

    const tear_out = useCallback((id: string, title: string) => {
        set_window(id, "window")
            .then(() => (is_tauri() ? invoke("open_pane_window", { sessionId: id, title }) : undefined))
            .then(() => list_windows().then(set_elsewhere))
            .catch(() => undefined);
    }, []);

    const shown = useMemo(
        () => (zoomed ? services.sessions.filter((entry) => entry.id === zoomed) : services.sessions),
        [services.sessions, zoomed],
    );

    const columns = useMemo(() => (shown.length > 4 ? 4 : Math.max(shown.length, 1)), [shown.length]);

    if (services.sessions.length === 0) {
        return (
            <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-1 p-4 text-center">
                <p className="font-mono text-[11px] text-shell">No terminal is open.</p>
                <p className="font-mono text-[10px] text-shade">
                    Start an agent, or open a shell from the header.
                </p>
            </div>
        );
    }

    return (
        <main
            className="grid min-h-0 min-w-0 flex-1 gap-1.5 p-1.5"
            style={{
                gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`,
                gridAutoRows: "minmax(0, 1fr)",
            }}
        >
            {shown.map((session) =>
                elsewhere[session.id] ? (
                    <article
                        key={session.id}
                        className="flex min-h-0 flex-col items-center justify-center gap-1 rounded-lg border border-dashed border-reef bg-lagoon-deep p-3 text-center"
                    >
                        <span className="text-[12px] text-shell">
                            {services.crew.find((agent) => agent.session_id === session.id)?.name ??
                                session.id}
                        </span>
                        <span className="font-mono text-[10px] text-shade">
                            open in its own window
                        </span>
                        <button
                            className="mt-1 rounded border border-reef px-2 py-0.5 font-mono text-[10px] text-shell hover:border-foam"
                            onClick={() => {
                                set_window(session.id, "grid")
                                    .then(() =>
                                        is_tauri()
                                            ? invoke("close_pane_window", { sessionId: session.id })
                                            : undefined,
                                    )
                                    .then(() => list_windows().then(set_elsewhere))
                                    .catch(() => undefined);
                            }}
                        >
                            bring it back
                        </button>
                    </article>
                ) : (
                <TerminalPane
                    key={session.id}
                    session={session}
                    label={services.crew.find((agent) => agent.session_id === session.id)?.name}
                    focused={
                        active &&
                        (services.focused_id
                            ? services.focused_id === session.id
                            : session.id === services.sessions[0]?.id)
                    }
                    on_focus={services.focus_pane}
                    on_close={services.close_session}
                    on_zoom={(id) => set_zoomed((held) => (held === id ? null : id))}
                    zoomed={zoomed === session.id}
                    on_branch={(entry) => entry.cwd && services.open_shell_in(entry.cwd)}
                    on_tear_out={(entry) =>
                        tear_out(
                            entry.id,
                            services.crew.find((agent) => agent.session_id === entry.id)?.name ??
                                entry.id,
                        )
                    }
                    on_metrics={services.on_metrics}
                />
                ),
            )}
        </main>
    );
}
