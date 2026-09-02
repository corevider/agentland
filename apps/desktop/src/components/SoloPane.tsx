import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import { Waiting } from "@/components/Spinner";
import { TerminalPane, type PaneMetrics } from "@/components/TerminalPane";
import { list_agents, list_sessions, set_window, type Agent, type SessionInfo } from "@/lib/core";

/// One pane, in a window of its own. The pty is the same one the grid was
/// showing; this is a second view of it, not a second terminal.
export function SoloPane({ session_id }: { session_id: string }) {
    const [session, set_session] = useState<SessionInfo | null>(null);
    const [crew, set_crew] = useState<Agent[]>([]);
    const [gone, set_gone] = useState(false);
    const [readable, set_readable] = useState(false);
    const metrics = useRef(new Map<string, PaneMetrics>());

    const refresh = useCallback(async () => {
        const [sessions, roster] = await Promise.all([list_sessions(), list_agents()]);
        const found = sessions.find((entry) => entry.id === session_id) ?? null;
        set_session(found);
        set_crew(roster);
        set_gone(!found);
    }, [session_id]);

    useEffect(() => {
        refresh().catch(() => undefined);
        const handle = window.setInterval(() => refresh().catch(() => undefined), 3000);
        return () => window.clearInterval(handle);
    }, [refresh]);

    useEffect(() => {
        const put_back = () => {
            void set_window(session_id, { holder: "grid" }).catch(() => undefined);
        };

        window.addEventListener("beforeunload", put_back);
        return () => window.removeEventListener("beforeunload", put_back);
    }, [session_id]);

    const held = crew.find((agent) => agent.session_id === session_id);
    const label = held?.name;

    return (
        <div className="flex h-screen flex-col bg-lagoon-deep text-linen">
            <header className="flex shrink-0 items-center gap-2 border-b border-reef/70 px-3 py-1.5">
                <span className="text-[13px] text-linen">{label ?? session_id}</span>
                <span className="font-mono text-[10px] text-shade">in its own window</span>
                <button
                    className="ml-auto rounded border border-reef px-2 py-0.5 font-mono text-[11px] text-shell hover:border-foam"
                    onClick={() => {
                        void set_window(session_id, { holder: "grid" })
                            .then(() => invoke("close_pane_window", { sessionId: session_id }))
                            .catch(() => undefined);
                    }}
                >
                    put it back
                </button>
            </header>

            <div className="flex min-h-0 flex-1 p-1.5">
                {session ? (
                    <TerminalPane
                        session={session}
                        label={label}
                        crowned={held?.role === "commander"}
                        focused
                        readable={readable}
                        on_readable={(wanted) => {
                            set_readable(wanted);
                            void set_window(session_id, { readable: wanted }).catch(() => undefined);
                        }}
                        on_focus={() => undefined}
                        on_metrics={(id, value) => metrics.current.set(id, value)}
                    />
                ) : (
                    <div className="flex flex-1 items-center justify-center font-mono text-[11px] text-shade">
                        {gone ? "this terminal has closed" : <Waiting says="opening…" />}
                    </div>
                )}
            </div>
        </div>
    );
}
