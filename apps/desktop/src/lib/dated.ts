/// When a card is from.
///
/// A card written before the board recorded dates has none of its own, but
/// its history often does: the commit, the review, the note that finished
/// it. The latest of those is a truer date than "no date" for a card that
/// was plainly worked on.
export function dated(at: number | undefined, evidence: { at?: number }[]): number {
    if (at) {
        return at;
    }

    return evidence.reduce((latest, entry) => Math.max(latest, entry.at ?? 0), 0);
}
