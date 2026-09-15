import type { Allowance } from "@/lib/core";
import { FIVE_HOURS } from "@/lib/logins";

/// The login a pane spends from, said the short way.
export function login_label(engine_id: string, account: string | null | undefined): string {
    const label = account?.trim();
    return label ? `${engine_id} · ${label}` : `${engine_id} · this machine's login`;
}

/// A login's five hours as a number worth showing, or null: not read yet, or
/// read so long ago that the window it was read from has come round.
export function five_hours_shown(allowance: Allowance | null): number | null {
    const spent = allowance?.session_percent;
    if (spent === undefined || spent === null) {
        return null;
    }
    if ((allowance?.read_seconds_ago ?? 0) >= FIVE_HOURS) {
        return null;
    }
    return spent;
}

/// A login's week as a number worth showing, or null when nothing has read it.
export function week_shown(allowance: Allowance | null): number | null {
    const spent = allowance?.weekly_percent;
    return spent === undefined || spent === null ? null : spent;
}
