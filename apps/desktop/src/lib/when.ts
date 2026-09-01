/// How this app says when something happened.
///
/// One way of saying it, everywhere: a record with no time is worse than an
/// awkward one, and three panels each inventing their own phrasing is how a
/// person ends up unable to tell which of two things came first. Recent times
/// are relative because that is what a person is asking ("did this just
/// happen?"); older ones become a date, because "412h ago" is not a thought
/// anybody has.
export function when(at: number, now: number): string {
    if (!at) {
        return "no date";
    }

    const seconds = now - at;

    if (seconds < 0) {
        return "just now";
    }
    if (seconds < 60) {
        return "just now";
    }
    if (seconds < 3600) {
        return `${Math.floor(seconds / 60)}m ago`;
    }
    if (seconds < 86400) {
        return `${Math.floor(seconds / 3600)}h ago`;
    }
    if (seconds < 7 * 86400) {
        return `${Math.floor(seconds / 86400)}d ago`;
    }

    return new Date(at * 1000).toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

/// The full stamp, for a tooltip: the relative form answers "recently?", this
/// answers "exactly when?".
export function exactly(at: number): string {
    return at ? new Date(at * 1000).toLocaleString() : "not recorded";
}

/// When something is next due, said the same way round.
export function due_in(at: number, now: number): string {
    if (!at) {
        return "not scheduled";
    }

    const seconds = at - now;
    if (seconds <= 0) {
        return "due now";
    }
    if (seconds < 60) {
        return "in under a minute";
    }
    if (seconds < 3600) {
        return `in ${Math.floor(seconds / 60)}m`;
    }
    if (seconds < 86400) {
        return `in ${Math.floor(seconds / 3600)}h`;
    }

    return `in ${Math.floor(seconds / 86400)}d`;
}
