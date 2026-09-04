import type { Ceilings, JournalEntry, Rate } from "@/lib/core";

/// The family a kind belongs to: `card.assigned` is a card thing.
export function family_of(kind: string): string {
    return kind.split(".")[0];
}

/// The families present, in the order they should be offered — most talked
/// about first, because a filter list sorted alphabetically buries the one
/// somebody actually wants.
export function families_in(entries: JournalEntry[]): string[] {
    const count = new Map<string, number>();
    for (const entry of entries) {
        const family = family_of(entry.kind);
        count.set(family, (count.get(family) ?? 0) + 1);
    }

    return [...count.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).map(([f]) => f);
}

/// Who did things, most active first, so "what did ada do last" is one click.
export function actors_in(entries: JournalEntry[]): string[] {
    const count = new Map<string, number>();
    for (const entry of entries) {
        if (entry.actor) {
            count.set(entry.actor, (count.get(entry.actor) ?? 0) + 1);
        }
    }

    return [...count.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).map(([a]) => a);
}

/// A number a person can hold in their head.
export function short_count(value: number): string {
    if (value < 1000) {
        return String(value);
    }
    if (value < 1_000_000) {
        return `${(value / 1000).toFixed(value < 10_000 ? 1 : 0)}k`;
    }
    return `${(value / 1_000_000).toFixed(2)}M`;
}

export interface Meter {
    label: string;
    used: number;
    ceiling: number;
    /// 0..1, clamped — a bar past its own end tells nobody anything.
    share: number;
    tightest: boolean;
}

/// The three ceilings as bars, with the one that decides marked.
///
/// The tightest is marked rather than sorted to the top: a row that moves around
/// is a row nobody can glance at twice and compare.
export function meters_of(rate: Rate, ceilings: Ceilings): Meter[] {
    const rows: Array<[string, number, number]> = [
        ["requests", rate.requests, ceilings.requests],
        ["input tokens", rate.input, ceilings.input],
        ["cached tokens", rate.cached, ceilings.cached],
        ["output tokens", rate.output, ceilings.output],
    ];

    const share = (used: number, ceiling: number) => (ceiling <= 0 ? 0 : Math.min(1, used / ceiling));
    const worst = Math.max(...rows.map(([, used, ceiling]) => share(used, ceiling)));

    return rows.map(([label, used, ceiling]) => ({
        label,
        used,
        ceiling,
        share: share(used, ceiling),
        // Nothing is "the tightest" when nothing has been spent.
        tightest: worst > 0 && share(used, ceiling) === worst,
    }));
}

/// How long ago, in the fewest words that are still true.
export function moments_ago(at: number, now: number): string {
    const seconds = Math.max(0, Math.round(now - at));
    if (seconds < 60) {
        return `${seconds}s`;
    }
    if (seconds < 3600) {
        return `${Math.round(seconds / 60)}m`;
    }
    if (seconds < 86_400) {
        return `${Math.round(seconds / 3600)}h`;
    }
    return `${Math.round(seconds / 86_400)}d`;
}

/// A rule as somebody would say it out loud.
///
/// The stored forms are the engine's — `Bash(npm test:*)`, `Dir(/tmp)` — and
/// reading a list of them is how a grant nobody understands gets left in place.
export function rule_reads(rule: string): string {
    const folder = rule.match(/^Dir\((.+)\)$/);
    if (folder) {
        return `reach anything under ${folder[1]}`;
    }

    const command = rule.match(/^Bash\((.+?)(:\*)?\)$/);
    if (command) {
        return `run ${command[1]}${command[2] ? "…" : ""}`;
    }

    return rule;
}
