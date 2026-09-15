import type {
    Routine,
    RoutineDay,
    RoutineDelivery,
    RoutinePayload,
    RoutineSchedule,
    RoutineTemplate,
} from "@/lib/core";

export const DAYS: RoutineDay[] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
export const WEEKDAYS: RoutineDay[] = ["mon", "tue", "wed", "thu", "fri"];
const WEEKEND: RoutineDay[] = ["sat", "sun"];

export const DAY_NAMES: Record<RoutineDay, string> = {
    mon: "Mon",
    tue: "Tue",
    wed: "Wed",
    thu: "Thu",
    fri: "Fri",
    sat: "Sat",
    sun: "Sun",
};

const DAY_PLURALS: Record<RoutineDay, string> = {
    mon: "Mondays",
    tue: "Tuesdays",
    wed: "Wednesdays",
    thu: "Thursdays",
    fri: "Fridays",
    sat: "Saturdays",
    sun: "Sundays",
};

export function in_order(days: RoutineDay[]): RoutineDay[] {
    return DAYS.filter((day) => days.includes(day));
}

function the_same(days: RoutineDay[], wanted: RoutineDay[]): boolean {
    return days.length === wanted.length && wanted.every((day) => days.includes(day));
}

/// Days the way a person says them, or null for every day.
///
/// "Mon, Tue, Wed, Thu, Fri" is read slower than "weekdays", and a run of three
/// or more in a row reads best as a range.
export function days_in_words(days: RoutineDay[]): string | null {
    const held = in_order(days);

    if (held.length === 0 || held.length === DAYS.length) {
        return null;
    }
    if (the_same(held, WEEKDAYS)) {
        return "weekdays";
    }
    if (the_same(held, WEEKEND)) {
        return "weekends";
    }
    if (held.length === 1) {
        return DAY_PLURALS[held[0]];
    }

    const positions = held.map((day) => DAYS.indexOf(day));
    const in_a_row = positions.every((at, index) => index === 0 || at === positions[index - 1] + 1);
    if (in_a_row && held.length >= 3) {
        return `${DAY_NAMES[held[0]]}–${DAY_NAMES[held[held.length - 1]]}`;
    }

    return held.map((day) => DAY_NAMES[day]).join(", ");
}

export function minutes_in_words(minutes: number): string {
    if (minutes === 60) {
        return "hour";
    }
    if (minutes > 60 && minutes % 60 === 0) {
        return `${minutes / 60} hours`;
    }
    return `${minutes} min`;
}

function listed(items: string[]): string {
    if (items.length <= 1) {
        return items.join("");
    }
    return `${items.slice(0, -1).join(", ")} and ${items[items.length - 1]}`;
}

/// A schedule in one line: "weekdays at 09:00", "every 4 hours, 09:00–19:00,
/// weekdays", "Fridays at 17:00".
export function schedule_in_words(schedule: RoutineSchedule): string {
    const days = days_in_words(schedule.days);

    if (schedule.kind === "daily") {
        return `${days ?? "daily"} at ${listed(schedule.times)}`;
    }

    const parts = [`every ${minutes_in_words(schedule.minutes)}`];
    if (schedule.from && schedule.to) {
        parts.push(`${schedule.from}–${schedule.to}`);
    }
    if (days) {
        parts.push(days);
    }
    return parts.join(", ");
}

export function valid_clock(text: string): boolean {
    return /^([01]?\d|2[0-3]):[0-5]\d$/.test(text.trim());
}

/// A time written the one way the core keeps it.
export function clock(text: string): string {
    const [hours, minutes] = text.trim().split(":");
    return `${hours.padStart(2, "0")}:${minutes}`;
}

/// Everything the editor holds while a routine is being written. Both shapes
/// of schedule are kept at once, so flipping between them loses nothing.
export interface RoutineDraft {
    name: string;
    agent_id: string;
    brief: string;
    kind: "every" | "daily";
    minutes: number;
    windowed: boolean;
    from: string;
    to: string;
    times: string[];
    days: RoutineDay[];
    delivery: RoutineDelivery;
    draft_only: boolean;
    skip_when_tight: boolean;
    one_at_a_time: boolean;
    pause_after_failures: number;
}

export const EMPTY_DRAFT: RoutineDraft = {
    name: "",
    agent_id: "",
    brief: "",
    kind: "daily",
    minutes: 60,
    windowed: false,
    from: "09:00",
    to: "18:00",
    times: ["09:00"],
    days: [...WEEKDAYS],
    delivery: "card",
    draft_only: false,
    skip_when_tight: true,
    one_at_a_time: true,
    pause_after_failures: 2,
};

function with_schedule(draft: RoutineDraft, schedule: RoutineSchedule): RoutineDraft {
    if (schedule.kind === "daily") {
        return { ...draft, kind: "daily", times: [...schedule.times], days: [...schedule.days] };
    }

    const windowed = Boolean(schedule.from && schedule.to);
    return {
        ...draft,
        kind: "every",
        minutes: schedule.minutes,
        windowed,
        from: windowed ? schedule.from ?? draft.from : draft.from,
        to: windowed ? schedule.to ?? draft.to : draft.to,
        days: [...schedule.days],
    };
}

export function draft_from_routine(routine: Routine): RoutineDraft {
    return with_schedule(
        {
            ...EMPTY_DRAFT,
            name: routine.name,
            agent_id: routine.agent_id,
            brief: routine.brief,
            delivery: routine.delivery,
            draft_only: routine.draft_only,
            skip_when_tight: routine.skip_when_tight,
            one_at_a_time: routine.one_at_a_time,
            pause_after_failures: routine.pause_after_failures,
        },
        routine.schedule,
    );
}

/// A template laid over what is already drafted. The agent stays: a template
/// says what to do and when, never who.
export function draft_from_template(template: RoutineTemplate, draft: RoutineDraft): RoutineDraft {
    return with_schedule(
        {
            ...draft,
            name: template.name,
            brief: template.brief,
            delivery: template.delivery,
            draft_only: template.draft_only,
            skip_when_tight: template.skip_when_tight,
            one_at_a_time: template.one_at_a_time,
        },
        template.schedule,
    );
}

export function schedule_of(draft: RoutineDraft): RoutineSchedule {
    const days = in_order(draft.days);

    if (draft.kind === "daily") {
        const times = [...new Set(draft.times.filter((time) => time.trim()).map(clock))].sort();
        return { kind: "daily", times, days };
    }

    return {
        kind: "every",
        minutes: Math.max(1, Math.round(draft.minutes)),
        from: draft.windowed ? clock(draft.from) : null,
        to: draft.windowed ? clock(draft.to) : null,
        days,
    };
}

/// What is wrong with a draft, said so it can be fixed, or null.
export function draft_problem(draft: RoutineDraft): string | null {
    if (!draft.name.trim()) {
        return "a routine needs a name";
    }
    if (!draft.agent_id) {
        return "pick the agent it runs on";
    }
    if (!draft.brief.trim()) {
        return "a routine needs a brief — what the agent is told each time";
    }

    if (draft.kind === "every") {
        if (!(draft.minutes >= 1)) {
            return "every how many minutes? at least one";
        }
        if (draft.windowed && (!valid_clock(draft.from) || !valid_clock(draft.to))) {
            return "the window needs both ends as HH:MM, like 09:00 and 18:00";
        }
        return null;
    }

    const times = draft.times.filter((time) => time.trim());
    if (times.length === 0) {
        return "a daily routine needs at least one time, like 09:00";
    }
    const wrong = times.find((time) => !valid_clock(time));
    if (wrong !== undefined) {
        return `"${wrong}" is not a time — write it as HH:MM, like 09:00`;
    }

    return null;
}

export function payload_of(draft: RoutineDraft): RoutinePayload {
    return {
        name: draft.name.trim(),
        agent_id: draft.agent_id,
        brief: draft.brief.trim(),
        schedule: schedule_of(draft),
        delivery: draft.delivery,
        draft_only: draft.draft_only,
        skip_when_tight: draft.skip_when_tight,
        one_at_a_time: draft.one_at_a_time,
        pause_after_failures: Math.max(1, Math.round(draft.pause_after_failures)),
    };
}

/// A `{variable}` put where the caret is, and where the caret goes after it.
export function insert_at(text: string, start: number, end: number, name: string): { text: string; caret: number } {
    const word = `{${name}}`;
    return {
        text: `${text.slice(0, start)}${word}${text.slice(end)}`,
        caret: start + word.length,
    };
}
