import { useEffect, useRef, useState } from "react";

import { mark_notices_seen, read_notices, type Notice, type NoticeReport } from "@/lib/core";
import { use_poll } from "@/lib/poll";

const TINT: Record<Notice["kind"], string> = {
    waiting: "text-coral",
    trouble: "text-coral",
    finished: "text-palm",
    word: "text-shell",
};

function how_long_ago(at: number, now: number): string {
    const seconds = Math.max(0, now - at);
    if (seconds < 60) {
        return `${seconds}s`;
    }
    if (seconds < 3600) {
        return `${Math.floor(seconds / 60)}m`;
    }
    return `${Math.floor(seconds / 3600)}h`;
}

/// The bell beside the settings button.
///
/// A crew working in parallel produces more than a person can watch, so what
/// reaches them has to say where it came from — which workspace, which agent —
/// and take them there. Only what changes what they would do next lights the
/// bell; finished work waits in the list.
export function NoticeBell({ on_open }: { on_open: (opens: string) => void }) {
    const [report, set_report] = useState<NoticeReport | null>(null);
    const [open, set_open] = useState(false);
    const [now, set_now] = useState(() => Math.floor(Date.now() / 1000));
    const holder = useRef<HTMLDivElement>(null);

    use_poll(() => {
        read_notices()
            .then(set_report)
            .catch(() => undefined);
        set_now(Math.floor(Date.now() / 1000));
    }, 4000);

    useEffect(() => {
        if (!open) {
            return;
        }

        const dismiss = (event: MouseEvent) => {
            if (!holder.current?.contains(event.target as Node)) {
                set_open(false);
            }
        };

        window.addEventListener("mousedown", dismiss);
        return () => window.removeEventListener("mousedown", dismiss);
    }, [open]);

    const unseen = report?.unseen ?? 0;
    const loud = report?.loud ?? false;

    return (
        <div ref={holder} className="relative flex items-center">
            <button
                className={`relative rounded border px-1.5 py-[3px] hover:border-turquoise hover:text-turquoise ${
                    loud ? "border-coral text-coral" : "border-reef text-driftwood"
                }`}
                title={
                    unseen === 0
                        ? "nothing new"
                        : `${unseen} new${loud ? " · someone is waiting on you" : ""}`
                }
                aria-label="Notices"
                onClick={() => {
                    const wanted = !open;
                    set_open(wanted);
                    if (wanted && unseen > 0) {
                        mark_notices_seen()
                            .then(() => read_notices().then(set_report))
                            .catch(() => undefined);
                    }
                }}
            >
                <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <path d="M18 8a6 6 0 1 0-12 0c0 7-3 9-3 9h18s-3-2-3-9" />
                    <path d="M13.7 21a2 2 0 0 1-3.4 0" />
                </svg>

                {unseen > 0 ? (
                    <span
                        className={`absolute -right-1 -top-1 min-w-[13px] rounded-full px-[3px] text-[9px] leading-[13px] tabular-nums ${
                            loud ? "bg-coral text-lagoon-deep" : "bg-reef text-linen"
                        }`}
                    >
                        {unseen > 99 ? "99+" : unseen}
                    </span>
                ) : null}
            </button>

            {open ? (
                <div className="absolute right-0 top-full z-50 mt-1 max-h-[60vh] w-[22rem] overflow-y-auto rounded-lg border border-foam bg-lagoon py-1 shadow-lg">
                    {(report?.notices.length ?? 0) === 0 ? (
                        <p className="px-3 py-2 font-mono text-[10px] text-shade">
                            Nothing yet. Agents asking for you, plans finishing and trouble land here.
                        </p>
                    ) : null}

                    {report?.notices.map((notice) => (
                        <button
                            key={notice.id}
                            className="block w-full px-3 py-1.5 text-left hover:bg-shallow"
                            onClick={() => {
                                set_open(false);
                                if (notice.opens) {
                                    on_open(notice.opens);
                                }
                            }}
                        >
                            <div className={`text-[11px] ${TINT[notice.kind]}`}>{notice.text}</div>
                            <div className="font-mono text-[9px] text-shade">
                                {[
                                    notice.agent_id,
                                    notice.repository_id,
                                    notice.workspace_id,
                                ]
                                    .filter(Boolean)
                                    .join(" · ") || notice.kind}
                                {" · "}
                                {how_long_ago(notice.at, now)} ago
                            </div>
                        </button>
                    ))}
                </div>
            ) : null}
        </div>
    );
}
