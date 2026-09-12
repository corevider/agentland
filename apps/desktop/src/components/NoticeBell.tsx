import { useEffect, useRef, useState } from "react";

import {
    mark_notices_seen,
    mark_notices_unseen,
    read_notices,
    set_desktop_notices,
    type Notice,
    type NoticeReport,
} from "@/lib/core";
import { with_marked } from "@/lib/notices";
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
///
/// Each notice stays unread until it is opened or marked read, and can be put
/// back as unread: opening the list is looking at it, not dealing with it.
export function NoticeBell({ on_open }: { on_open: (opens: string) => void }) {
    const [report, set_report] = useState<NoticeReport | null>(null);
    const [open, set_open] = useState(false);
    const [switching, set_switching] = useState(false);
    const [now, set_now] = useState(() => Math.floor(Date.now() / 1000));
    const holder = useRef<HTMLDivElement>(null);

    const refresh = () =>
        read_notices()
            .then(set_report)
            .catch(() => undefined);

    use_poll(() => {
        void refresh();
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

    const mark = (ids: number[], read: boolean) => {
        set_report((held) => (held ? with_marked(held, ids, read) : held));
        return (read ? mark_notices_seen(ids) : mark_notices_unseen(ids))
            .then(refresh)
            .catch(() => void refresh());
    };

    const unseen = report?.unseen ?? 0;
    const loud = report?.loud ?? false;
    const desktop = report?.desktop ?? true;

    const switch_desktop = () => {
        const wanted = !desktop;
        set_switching(true);
        set_report((held) => (held ? { ...held, desktop: wanted } : held));
        set_desktop_notices(wanted)
            .then(set_report)
            .catch(() => void refresh())
            .finally(() => set_switching(false));
    };

    return (
        <div ref={holder} className="relative flex items-center">
            <button
                className={`relative rounded border px-1.5 py-[3px] hover:border-turquoise hover:text-turquoise ${
                    loud ? "border-coral text-coral" : "border-reef text-driftwood"
                }`}
                title={
                    unseen === 0
                        ? "nothing unread"
                        : `${unseen} unread${loud ? " · someone is waiting on you" : ""}`
                }
                aria-label="Notices"
                onClick={() => set_open(!open)}
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
                <div className="absolute right-0 top-full z-50 mt-1 max-h-[60vh] w-[24rem] overflow-y-auto rounded-lg border border-foam bg-lagoon py-1 shadow-lg">
                    <div className="flex items-center justify-between gap-2 border-b border-reef px-3 pb-1.5 pt-0.5 font-mono text-[10px]">
                        <span className="text-shade">{unseen === 0 ? "nothing unread" : `${unseen} unread`}</span>
                        <span className="flex gap-1.5">
                            <button
                                className="rounded border border-reef px-1.5 text-shell hover:border-turquoise hover:text-turquoise disabled:opacity-40"
                                disabled={unseen === 0}
                                onClick={() => void mark([], true)}
                            >
                                mark all read
                            </button>
                            <button
                                className={`rounded border px-1.5 hover:border-turquoise hover:text-turquoise disabled:opacity-60 ${
                                    desktop ? "border-palm text-palm" : "border-reef text-shade"
                                }`}
                                title="show waiting, trouble and finished notices on the desktop while this window is not in front"
                                disabled={switching}
                                onClick={switch_desktop}
                            >
                                desktop {desktop ? "on" : "off"}
                            </button>
                        </span>
                    </div>

                    {(report?.notices.length ?? 0) === 0 ? (
                        <p className="px-3 py-2 font-mono text-[10px] text-shade">
                            Nothing yet. Agents asking for you, plans finishing and trouble land here.
                        </p>
                    ) : null}

                    {report?.notices.map((notice) => (
                        <div
                            key={notice.id}
                            data-notice={notice.id}
                            className={`group flex items-start gap-2 px-3 py-1.5 hover:bg-shallow ${
                                notice.seen ? "opacity-60" : ""
                            }`}
                        >
                            <button
                                className="min-w-0 flex-1 text-left"
                                onClick={() => {
                                    set_open(false);
                                    if (!notice.seen) {
                                        void mark([notice.id], true);
                                    }
                                    if (notice.opens) {
                                        on_open(notice.opens);
                                    }
                                }}
                            >
                                <div className={`text-[11px] ${TINT[notice.kind]} ${notice.seen ? "" : "font-semibold"}`}>
                                    {notice.text}
                                </div>
                                <div className="font-mono text-[9px] text-shade">
                                    {[notice.agent_id, notice.repository_id, notice.workspace_id]
                                        .filter(Boolean)
                                        .join(" · ") || notice.kind}
                                    {" · "}
                                    {how_long_ago(notice.at, now)} ago
                                </div>
                            </button>

                            <button
                                className="mt-1 flex h-3 w-3 shrink-0 items-center justify-center rounded-full border border-reef hover:border-turquoise"
                                title={notice.seen ? "mark as unread" : "mark as read"}
                                aria-label={notice.seen ? "mark as unread" : "mark as read"}
                                onClick={() => void mark([notice.id], !notice.seen)}
                            >
                                {notice.seen ? null : <span className="h-1.5 w-1.5 rounded-full bg-turquoise" />}
                            </button>
                        </div>
                    ))}
                </div>
            ) : null}
        </div>
    );
}
