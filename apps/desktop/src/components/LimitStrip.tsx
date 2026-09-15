import type { Allowance } from "@/lib/core";
import { nearness, out_words } from "@/lib/logins";
import { five_hours_shown, week_shown } from "@/lib/strip";

const COLOUR: Record<Allowance["room"], string> = {
    plenty: "bg-palm",
    tight: "bg-sun",
    spent: "bg-coral",
};

/// One allowance, small: a label, a bar with the switch point marked, a number.
function Small({
    label,
    percent,
    point,
    colour,
    title,
}: {
    label: string;
    percent: number | null;
    point: number;
    colour: Allowance["room"];
    title: string;
}) {
    return (
        <span className="flex shrink-0 items-center gap-1" title={title}>
            <span className="text-shade">{label}</span>
            <span className="relative h-1 w-12 overflow-hidden rounded-full bg-reef/60">
                {percent !== null ? (
                    <span
                        className={`block h-full ${COLOUR[colour]}`}
                        style={{ width: `${Math.min(100, Math.max(0, percent))}%` }}
                    />
                ) : null}
                <span className="absolute top-0 h-full w-px bg-linen/60" style={{ left: `${point}%` }} />
            </span>
            <span className="w-8 tabular-nums text-shell">{percent === null ? "—" : `${Math.round(percent)}%`}</span>
        </span>
    );
}

/// The limits of the login an agent's pane spends from, along the pane's foot:
/// its five hours on the left, its week on the right, and which login it is
/// between them — or, when its engine has said it is out, when it comes back.
///
/// The Logins page says all of this for every login at once, but it is a page
/// away, and the question comes up while watching the pane: is this one about
/// to stop, and whose week is it spending?
export function LimitStrip({
    allowance,
    login,
    week_at,
    five_hours_at,
    now,
    on_hide,
}: {
    allowance: Allowance | null;
    login: string;
    week_at: number;
    five_hours_at: number;
    now: number;
    on_hide: () => void;
}) {
    const five_hours = five_hours_shown(allowance);
    const week = week_shown(allowance);
    const out = out_words(allowance, now);
    const out_on_five_hours = out !== null && allowance?.limit_window === "session";

    return (
        <div className="flex shrink-0 items-center gap-2 border-t border-reef/50 px-2 py-[2px] font-mono text-[10px]">
            <Small
                label="5h"
                percent={out_on_five_hours ? 100 : five_hours}
                point={five_hours_at}
                colour={out_on_five_hours ? "spent" : nearness(five_hours, five_hours_at)}
                title={
                    five_hours === null
                        ? "this login's five hours — not read yet, or come round since it was last read"
                        : `this login's five hours; agents are moved on at ${five_hours_at}%`
                }
            />
            <span
                className={`min-w-0 flex-1 truncate text-center ${out ? "text-coral" : "text-shell"}`}
                title={out ? `${login} — ${out}` : `spending from ${login}`}
            >
                {out ?? login}
            </span>
            <Small
                label="week"
                percent={week}
                point={week_at}
                colour={allowance?.room === "spent" ? "spent" : nearness(week, week_at)}
                title={
                    week === null
                        ? "this login's week — not read yet"
                        : `this login's week; agents are moved on at ${week_at}%`
                }
            />
            <button
                className="shrink-0 px-0.5 text-shade hover:text-linen"
                title="hide the limits under every pane — Settings › Terminal brings them back"
                onClick={(event) => {
                    event.stopPropagation();
                    on_hide();
                }}
            >
                ×
            </button>
        </div>
    );
}
