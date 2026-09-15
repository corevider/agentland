import { useRef, useState } from "react";

import type { RoutineDay, RoutineTemplate } from "@/lib/core";
import { Picker } from "@/components/Picker";
import { Press } from "@/components/Press";
import {
    DAYS,
    DAY_NAMES,
    WEEKDAYS,
    draft_from_template,
    draft_problem,
    insert_at,
    schedule_in_words,
    schedule_of,
    valid_clock,
    type RoutineDraft,
} from "@/lib/routines";

const INPUT = "rounded-md border border-reef bg-lagoon-deep px-1.5 py-0.5 font-mono text-[11px] text-linen";
const LABEL = "w-14 shrink-0 font-mono text-[10px] uppercase tracking-[0.12em] text-shade";

function Choice<T extends string>({
    value,
    options,
    on_pick,
}: {
    value: T;
    options: { value: T; label: string; says: string }[];
    on_pick: (value: T) => void;
}) {
    return (
        <span className="inline-flex overflow-hidden rounded-md border border-reef">
            {options.map((option) => (
                <button
                    key={option.value}
                    type="button"
                    title={option.says}
                    className={`px-2 py-0.5 font-mono text-[11px] ${
                        value === option.value ? "bg-reef text-linen" : "text-shade hover:text-shell"
                    }`}
                    onClick={() => on_pick(option.value)}
                >
                    {option.label}
                </button>
            ))}
        </span>
    );
}

function Tick({
    checked,
    on_change,
    label,
    says,
}: {
    checked: boolean;
    on_change: (checked: boolean) => void;
    label: string;
    says: string;
}) {
    return (
        <label className="flex items-center gap-1 font-mono text-[11px] text-shell" title={says}>
            <input type="checkbox" checked={checked} onChange={(event) => on_change(event.target.checked)} />
            {label}
        </label>
    );
}

/// The schedule in words, once it can be read as one.
function summary_of(draft: RoutineDraft): string | null {
    if (draft.kind === "daily") {
        const times = draft.times.filter((time) => time.trim());
        return times.length > 0 && times.every(valid_clock) ? schedule_in_words(schedule_of(draft)) : null;
    }

    const window_reads = !draft.windowed || (valid_clock(draft.from) && valid_clock(draft.to));
    return draft.minutes >= 1 && window_reads ? schedule_in_words(schedule_of(draft)) : null;
}

/// Writing a routine, new or old.
///
/// The one-row form it replaces could say "every N minutes" and nothing else,
/// so a morning triage ran at whatever minute the app was opened. Everything a
/// routine can be is on the page here, and what it will do is said back in one
/// line as it is written, so nobody has to work out what "240 min, 09:00–19:00,
/// Mon–Fri" comes to.
export function RoutineEditor({
    initial,
    agents,
    templates,
    variables,
    saving_label,
    on_save,
    on_cancel,
}: {
    initial: RoutineDraft;
    agents: { value: string; label: string }[];
    templates: RoutineTemplate[];
    variables: { name: string; says: string }[];
    saving_label: string;
    /// Waited for: the save button says it is saving until this settles.
    on_save: (draft: RoutineDraft) => unknown;
    on_cancel: () => void;
}) {
    const [draft, set_draft] = useState<RoutineDraft>(initial);
    const [tried, set_tried] = useState(false);
    const brief_ref = useRef<HTMLTextAreaElement>(null);

    const change = (patch: Partial<RoutineDraft>) => set_draft((held) => ({ ...held, ...patch }));
    const problem = draft_problem(draft);
    const summary = summary_of(draft);

    const toggle_day = (day: RoutineDay) =>
        change({
            days: draft.days.includes(day) ? draft.days.filter((held) => held !== day) : [...draft.days, day],
        });

    const put_variable = (name: string) => {
        const box = brief_ref.current;
        const start = box?.selectionStart ?? draft.brief.length;
        const end = box?.selectionEnd ?? start;
        const placed = insert_at(draft.brief, start, end, name);

        change({ brief: placed.text });
        requestAnimationFrame(() => {
            box?.focus();
            box?.setSelectionRange(placed.caret, placed.caret);
        });
    };

    const save = () => {
        set_tried(true);
        if (!problem) {
            return on_save(draft);
        }
    };

    return (
        <section className="flex flex-col gap-2 rounded-md border border-turquoise/60 bg-lagoon-deep px-2.5 py-2">
            {templates.length > 0 ? (
                <div className="flex flex-wrap items-center gap-1">
                    <span className={LABEL}>start</span>
                    {templates.map((template) => (
                        <button
                            key={template.id}
                            type="button"
                            title={template.summary}
                            className="rounded border border-reef px-1.5 font-mono text-[10px] text-shell hover:border-turquoise hover:text-turquoise"
                            onClick={() => set_draft((held) => draft_from_template(template, held))}
                        >
                            {template.name}
                            {template.suits === "commander" ? (
                                <span className="text-shade"> · commander</span>
                            ) : null}
                        </button>
                    ))}
                </div>
            ) : null}

            <div className="flex flex-wrap items-center gap-1.5">
                <span className={LABEL}>what</span>
                <input
                    className={`${INPUT} min-w-[140px] flex-1`}
                    placeholder="name"
                    value={draft.name}
                    onChange={(event) => change({ name: event.target.value })}
                />
                <Picker
                    className="rounded-md border border-reef bg-lagoon-deep px-[7px] py-[3px] font-mono text-[11px]"
                    value={draft.agent_id}
                    placeholder="nobody here yet"
                    choices={agents}
                    on_pick={(held) => change({ agent_id: held })}
                />
            </div>

            <div className="flex flex-wrap items-center gap-1.5">
                <span className={LABEL}>when</span>
                <Choice
                    value={draft.kind}
                    on_pick={(kind) => change({ kind })}
                    options={[
                        { value: "daily", label: "at set times", says: "for work a person expects at a moment, like a morning triage" },
                        { value: "every", label: "every so often", says: "for a check that should keep happening, like a flaky test hunt" },
                    ]}
                />

                {draft.kind === "daily" ? (
                    <span className="flex flex-wrap items-center gap-1">
                        {draft.times.map((time, index) => (
                            <span key={index} className="flex items-center">
                                <input
                                    className={`${INPUT} w-[58px] ${time.trim() && !valid_clock(time) ? "border-coral" : ""}`}
                                    placeholder="09:00"
                                    value={time}
                                    onChange={(event) =>
                                        change({
                                            times: draft.times.map((held, at) => (at === index ? event.target.value : held)),
                                        })
                                    }
                                />
                                {draft.times.length > 1 ? (
                                    <button
                                        type="button"
                                        className="px-1 font-mono text-[11px] text-shade hover:text-coral"
                                        title="take this time out"
                                        onClick={() => change({ times: draft.times.filter((_, at) => at !== index) })}
                                    >
                                        ×
                                    </button>
                                ) : null}
                            </span>
                        ))}
                        <button
                            type="button"
                            className="rounded border border-reef px-1.5 font-mono text-[10px] text-shell hover:border-foam"
                            onClick={() => change({ times: [...draft.times, ""] })}
                        >
                            + time
                        </button>
                    </span>
                ) : (
                    <span className="flex flex-wrap items-center gap-1 font-mono text-[11px] text-shell">
                        every
                        <input
                            type="number"
                            min={1}
                            className={`${INPUT} w-16`}
                            value={draft.minutes}
                            onChange={(event) => change({ minutes: Number(event.target.value) || 0 })}
                        />
                        min
                        <label className="ml-1 flex items-center gap-1" title="outside these hours it waits for the window to open">
                            <input
                                type="checkbox"
                                checked={draft.windowed}
                                onChange={(event) => change({ windowed: event.target.checked })}
                            />
                            only between
                        </label>
                        <input
                            className={`${INPUT} w-[58px]`}
                            disabled={!draft.windowed}
                            value={draft.from}
                            onChange={(event) => change({ from: event.target.value })}
                        />
                        and
                        <input
                            className={`${INPUT} w-[58px]`}
                            disabled={!draft.windowed}
                            value={draft.to}
                            onChange={(event) => change({ to: event.target.value })}
                        />
                    </span>
                )}
            </div>

            <div className="flex flex-wrap items-center gap-1">
                <span className={LABEL}>days</span>
                {DAYS.map((day) => (
                    <button
                        key={day}
                        type="button"
                        className={`w-9 rounded border py-0.5 font-mono text-[10px] ${
                            draft.days.includes(day) ? "border-turquoise text-turquoise" : "border-reef text-shade"
                        }`}
                        onClick={() => toggle_day(day)}
                    >
                        {DAY_NAMES[day]}
                    </button>
                ))}
                <button
                    type="button"
                    className="ml-1 rounded border border-reef px-1.5 font-mono text-[10px] text-shell hover:border-foam"
                    onClick={() => change({ days: [...WEEKDAYS] })}
                >
                    weekdays
                </button>
                <button
                    type="button"
                    className="rounded border border-reef px-1.5 font-mono text-[10px] text-shell hover:border-foam"
                    title="no day picked means every day"
                    onClick={() => change({ days: [] })}
                >
                    every day
                </button>
                <span className="ml-auto font-mono text-[10px] text-driftwood">{summary ?? ""}</span>
            </div>

            <div className="flex flex-wrap items-center gap-1.5">
                <span className={LABEL}>goes</span>
                <Choice
                    value={draft.delivery}
                    on_pick={(delivery) => change({ delivery })}
                    options={[
                        { value: "card", label: "on a new card", says: "for work that produces a change and should be reviewed" },
                        { value: "pane", label: "into its pane", says: "for a commander or a chief whose job on a timer is to look around and decide" },
                    ]}
                />
                <span className="font-mono text-[10px] text-shade">
                    {draft.delivery === "card"
                        ? "a card on the agent's project, handed to it"
                        : "said to the agent where it stands, when it is at rest — no card"}
                </span>
            </div>

            <div className="flex flex-col gap-1">
                <textarea
                    ref={brief_ref}
                    rows={4}
                    className={`${INPUT} w-full resize-y leading-snug`}
                    placeholder="what it should do each time — written to be read cold"
                    value={draft.brief}
                    onChange={(event) => change({ brief: event.target.value })}
                />
                {variables.length > 0 ? (
                    <div className="flex flex-wrap items-center gap-1">
                        <span className="font-mono text-[10px] text-shade">filled in each run:</span>
                        {variables.map((variable) => (
                            <button
                                key={variable.name}
                                type="button"
                                title={variable.says}
                                className="rounded border border-reef px-1 font-mono text-[10px] text-driftwood hover:border-turquoise hover:text-turquoise"
                                onClick={() => put_variable(variable.name)}
                            >
                                {`{${variable.name}}`}
                            </button>
                        ))}
                    </div>
                ) : null}
            </div>

            <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                <Tick
                    checked={draft.skip_when_tight}
                    on_change={(value) => change({ skip_when_tight: value })}
                    label="skip when the week is tight"
                    says="leave a run out while the agent's allowance says to start nothing new"
                />
                {draft.delivery === "card" ? (
                    <Tick
                        checked={draft.one_at_a_time}
                        on_change={(value) => change({ one_at_a_time: value })}
                        label="one card at a time"
                        says="leave a run out while the card from the last one is still open"
                    />
                ) : null}
                <Tick
                    checked={draft.draft_only}
                    on_change={(value) => change({ draft_only: value })}
                    label="draft only"
                    says="the agent prepares the work and stops before anything leaves this machine"
                />
                <label className="flex items-center gap-1 font-mono text-[11px] text-shell" title="failures in a row before it pauses itself">
                    pause after
                    <input
                        type="number"
                        min={1}
                        className={`${INPUT} w-12`}
                        value={draft.pause_after_failures}
                        onChange={(event) => change({ pause_after_failures: Number(event.target.value) || 1 })}
                    />
                    failures
                </label>
            </div>

            <div className="flex items-center gap-2">
                {tried && problem ? <span className="font-mono text-[11px] text-coral">{problem}</span> : null}
                <span className="ml-auto flex gap-1.5">
                    <button
                        type="button"
                        className="rounded-md border border-reef px-2 py-0.5 font-mono text-[11px] text-shell hover:border-foam"
                        onClick={on_cancel}
                    >
                        cancel
                    </button>
                    <Press
                        className="rounded-md border border-turquoise px-2 py-0.5 font-mono text-[11px] text-turquoise"
                        busy_says="saving…"
                        on_press={save}
                    >
                        {saving_label}
                    </Press>
                </span>
            </div>
        </section>
    );
}
