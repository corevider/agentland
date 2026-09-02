import type { Holdings } from "@/lib/core";

/// What an agent is holding, said plainly, one line per thing.
///
/// Counting is not the point — a card left pointing at somebody who is gone is
/// what this is here to prevent, so each line names what would be lost rather
/// than summing them into a number.
export function what_is_held(holdings: Holdings): string[] {
    const lines: string[] = [];

    for (const card of holdings.cards) {
        lines.push(`${card.id} · ${card.title} — ${card.column}, goes back to the board`);
    }

    if (holdings.pane_running) {
        lines.push("its pane is open and will be closed");
    }

    if (holdings.uncommitted > 0) {
        lines.push(
            `${holdings.uncommitted} file${holdings.uncommitted === 1 ? "" : "s"} changed and never committed`,
        );
    }

    if (holdings.unpushed > 0) {
        lines.push(
            `${holdings.unpushed} commit${holdings.unpushed === 1 ? "" : "s"} on its branch and nowhere else`,
        );
    }

    return lines;
}
