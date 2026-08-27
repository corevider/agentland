import { useMemo, useState } from "react";

import { TerminalPane } from "@/components/TerminalPane";
import { use_services } from "@/workspace/registry";

export function TerminalsPanel({ active }: { active: boolean }) {
    const services = use_services();
    const [zoomed, set_zoomed] = useState<string | null>(null);

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
            {shown.map((session) => (
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
                    on_metrics={services.on_metrics}
                />
            ))}
        </main>
    );
}
