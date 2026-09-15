import type { Account, AccountsReport, Agent, Allowance, JournalEntry } from "@/lib/core";

/// One login and what is happening on it.
export interface LoginRow {
    /// `claude` for the machine's own login, `claude/second` for one added here.
    identity: string;
    engine_id: string;
    /// Null for the login the machine itself is signed in as.
    label: string | null;
    /// Null for the machine's own login, which Agentland did not add and does
    /// not hold a folder for.
    account: Account | null;
    allowance: Allowance | null;
    /// The names of the agents spending from it now.
    agents: string[];
}

export function identity_of(engine_id: string, label: string | null): string {
    return label ? `${engine_id}/${label}` : engine_id;
}

/// Every login worth a row, each engine's together, in the person's order.
///
/// The logins added here are only half of it. The machine's own login is
/// where the crew spends until somebody says otherwise, and it is the one that
/// runs out first — a panel that leaves it off shows a second subscription with
/// nothing on it and never says why anybody would want one. So an engine with
/// a login added, or with anything spent on it, gets its own login as a row,
/// and it takes its place in the order like any other.
export function login_rows(report: AccountsReport, allowances: Allowance[], crew: Agent[]): LoginRow[] {
    const name_of = (id: string) => crew.find((agent) => agent.id === id)?.name ?? id;
    const by_identity = new Map(allowances.map((allowance) => [allowance.identity, allowance]));
    const order = report.order ?? [];
    const rank = (identity: string) => {
        const at = order.indexOf(identity);
        return at < 0 ? Number.MAX_SAFE_INTEGER : at;
    };
    const engines = [
        ...new Set([
            ...report.accounts.map((account) => account.engine_id),
            ...allowances.map((allowance) => allowance.identity.split("/")[0]),
        ]),
    ].sort();

    const rows: LoginRow[] = [];
    for (const engine_id of engines) {
        const own = by_identity.get(engine_id) ?? null;
        const mine: LoginRow[] = [
            {
                identity: engine_id,
                engine_id,
                label: null,
                account: null,
                allowance: own,
                agents: (own?.agents ?? []).map(name_of),
            },
        ];

        for (const account of report.accounts.filter((held) => held.engine_id === engine_id)) {
            const identity = identity_of(engine_id, account.label);
            const allowance = by_identity.get(identity) ?? null;
            mine.push({
                identity,
                engine_id,
                label: account.label,
                account,
                allowance,
                agents: (allowance?.agents ?? []).map(name_of),
            });
        }

        rows.push(...mine.sort((first, second) => rank(first.identity) - rank(second.identity)));
    }

    return rows;
}

/// The order with one login moved a place up or down among its engine's. A
/// login is only ever traded for another on the same engine: an agent cannot
/// be carried from Claude to Codex, so ranking across them means nothing.
export function reorder(rows: LoginRow[], identity: string, by: -1 | 1): string[] {
    const order = rows.map((row) => row.identity);
    const at = order.indexOf(identity);
    const to = at + by;

    if (at < 0 || to < 0 || to >= rows.length || rows[to].engine_id !== rows[at].engine_id) {
        return order;
    }

    [order[at], order[to]] = [order[to], order[at]];
    return order;
}

/// Where a login stands among its engine's, counting from one.
export function rank_among(rows: LoginRow[], row: LoginRow): { rank: number; of: number } {
    const same = rows.filter((held) => held.engine_id === row.engine_id);
    return { rank: same.findIndex((held) => held.identity === row.identity) + 1, of: same.length };
}

export function ordinal(rank: number): string {
    const tens = rank % 100;
    if (tens >= 11 && tens <= 13) {
        return `${rank}th`;
    }
    return `${rank}${{ 1: "st", 2: "nd", 3: "rd" }[rank % 10] ?? "th"}`;
}

/// How long a five-hour window lasts, in seconds.
export const FIVE_HOURS = 5 * 60 * 60;

/// How much of a login's five hours is gone, or why that is not known now.
///
/// A login is read only while a pane runs on it. One everybody has left keeps
/// its last number, and a number older than the window it was read from says
/// nothing — the core ignores it too, so the panel says so rather than showing
/// a login as full that has long since come round.
export function five_hours_words(allowance: Allowance | null): string {
    const spent = allowance?.session_percent;
    if (spent === undefined || spent === null) {
        return "five hours not read yet";
    }
    if ((allowance?.read_seconds_ago ?? 0) >= FIVE_HOURS) {
        return "five hours come round since it was last read";
    }
    return `${Math.round(spent)}% of its five hours`;
}

/// A wait said the way a person says one: "1h 12m", "2d 3h", "under a minute".
export function wait_words(seconds: number): string {
    if (seconds < 60) {
        return "under a minute";
    }

    const minutes = Math.round(seconds / 60);
    const days = Math.floor(minutes / 1440);
    const hours = Math.floor((minutes % 1440) / 60);
    const rest = minutes % 60;

    if (days > 0) {
        return hours > 0 ? `${days}d ${hours}h` : `${days}d`;
    }
    if (hours > 0) {
        return rest > 0 ? `${hours}h ${rest}m` : `${hours}h`;
    }
    return `${rest}m`;
}

/// When a login its engine has said is out comes back, or null when it is
/// not out. The time is the engine's own — "resets 3pm" — so it is a promise,
/// where a percentage is only a reading.
export function out_words(allowance: Allowance | null, now: number): string | null {
    const back = allowance?.limit_back_at;
    if (!back || back <= now) {
        return null;
    }

    const wall =
        allowance?.limit_window === "session"
            ? " on its five hours"
            : allowance?.limit_window === "weekly"
              ? " on its week"
              : "";
    return `out${wall} — back in ${wait_words(back - now)}`;
}

/// A bar's colour: past the point it is moved on at, close to it, or clear.
export function nearness(percent: number | null | undefined, point: number): "plenty" | "tight" | "spent" {
    if (percent === undefined || percent === null) {
        return "plenty";
    }
    if (percent >= point) {
        return "spent";
    }
    return percent >= point - 15 ? "tight" : "plenty";
}

/// How much of the week is gone, or why that is not known yet.
export function week_words(allowance: Allowance | null): string {
    const spent = allowance?.weekly_percent;
    if (spent === undefined || spent === null) {
        return "week not read yet — it is once a pane on this login has been open a moment";
    }
    return `${Math.round(spent)}% of the week spent`;
}

export function who_spends(agents: string[]): string {
    if (agents.length === 0) {
        return "nobody on it now";
    }

    const names =
        agents.length === 1 ? agents[0] : `${agents.slice(0, -1).join(", ")} and ${agents[agents.length - 1]}`;
    return `${names} ${agents.length === 1 ? "spends" : "spend"} from it`;
}

/// The last few times an agent was carried from a spent login to another,
/// newest first.
export function hand_overs(entries: JournalEntry[], most = 3): JournalEntry[] {
    return entries
        .filter((entry) => entry.kind === "accounts.handed_over")
        .sort((first, second) => second.at - first.at)
        .slice(0, most);
}
