/// Where an update stands, as one value.
///
/// Checking, finding, downloading and installing are four different waits and a
/// person watching them wants a different sentence for each. Keeping them in one
/// union is what stops the panel showing "checking…" next to a finished download.
export type UpdateState =
    | { kind: "idle" }
    | { kind: "checking" }
    /// Checked, and this is the newest there is.
    | { kind: "current"; at: number }
    | { kind: "available"; version: string; notes: string; date: string | null }
    | { kind: "downloading"; got: number; total: number | null }
    /// Downloaded and installed; it takes effect when the app restarts.
    | { kind: "ready"; version: string }
    /// There is nowhere to check. A development build, or a browser.
    | { kind: "off"; why: string }
    | { kind: "trouble"; why: string };

export function bytes_in_words(count: number): string {
    if (count < 1024) {
        return `${count} B`;
    }
    if (count < 1024 * 1024) {
        return `${(count / 1024).toFixed(0)} KB`;
    }
    return `${(count / (1024 * 1024)).toFixed(1)} MB`;
}

/// How far a download has got.
///
/// A total is not always sent — the server may not say how big the file is — and
/// a percentage of an unknown total is a number made up. So it says what it
/// knows and no more.
export function progress_line(got: number, total: number | null): string {
    if (total === null || total <= 0) {
        return `${bytes_in_words(got)} so far`;
    }

    const percent = Math.min(100, Math.round((got / total) * 100));
    return `${bytes_in_words(got)} of ${bytes_in_words(total)} · ${percent}%`;
}

/// The one line the panel shows for where things stand.
export function in_a_sentence(state: UpdateState, now: number): string {
    switch (state.kind) {
        case "idle":
            return "Not checked yet.";
        case "checking":
            return "Asking whether there is a newer one…";
        case "current": {
            const ago = Math.max(0, Math.round((now - state.at) / 1000));
            return ago < 60
                ? "This is the newest there is."
                : `This is the newest there is, as of ${Math.round(ago / 60)} minute${ago >= 120 ? "s" : ""} ago.`;
        }
        case "available":
            return `Version ${state.version} is out.`;
        case "downloading":
            return `Downloading · ${progress_line(state.got, state.total)}`;
        case "ready":
            return `Version ${state.version} is installed. It takes effect when the app restarts.`;
        case "off":
            return state.why;
        case "trouble":
            return state.why;
    }
}

/// Whether the button that starts a download should be offered.
export function can_install(state: UpdateState): boolean {
    return state.kind === "available";
}

/// Whether checking again makes sense right now.
export function can_check(state: UpdateState): boolean {
    return state.kind !== "checking" && state.kind !== "downloading" && state.kind !== "off";
}

/// Release notes as the forge sent them, trimmed to what a panel can hold.
///
/// A release body can be the whole changelog. The panel shows the top of it and
/// says it is showing the top, rather than silently cutting somebody off
/// mid-sentence.
export function notes_for_reading(notes: string, lines: number): { text: string; trimmed: boolean } {
    const kept = notes.trim().split("\n");
    if (kept.length <= lines) {
        return { text: kept.join("\n"), trimmed: false };
    }

    return { text: kept.slice(0, lines).join("\n"), trimmed: true };
}
