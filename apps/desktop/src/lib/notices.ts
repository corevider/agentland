import type { NoticeReport } from "@/lib/core";

/// The bell as it will be once these notices are marked, drawn before the core
/// answers.
///
/// Waiting for the answer left the old state on screen for as long as the core
/// took to save, and a second click on what still looked unchanged undid the
/// first. Naming no notices means every one of them, and only for reading.
export function with_marked(report: NoticeReport, ids: number[], read: boolean): NoticeReport {
    const all = read && ids.length === 0;
    const touched = (id: number) => all || ids.includes(id);

    let change = 0;
    const notices = report.notices.map((notice) => {
        if (!touched(notice.id) || notice.seen === read) {
            return notice;
        }
        change += read ? -1 : 1;
        return { ...notice, seen: read };
    });

    const unseen = all ? 0 : Math.max(0, report.unseen + change);
    const loud_left = notices.some((notice) => !notice.seen && (notice.kind === "waiting" || notice.kind === "trouble"));

    return {
        ...report,
        notices,
        unseen,
        loud: unseen === 0 ? false : read ? report.loud && loud_left : report.loud || loud_left,
    };
}
