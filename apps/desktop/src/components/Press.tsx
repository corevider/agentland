import { useState, type ButtonHTMLAttributes, type MouseEvent, type ReactNode } from "react";

import { Spinner } from "@/components/Spinner";
import { one_press_at_a_time } from "@/lib/pending";

/// How long a failed press says so on itself before it looks like a button again.
const FAILED_FOR = 6000;

type Props = Omit<ButtonHTMLAttributes<HTMLButtonElement>, "onClick"> & {
    /// The work. Whatever it returns is waited for, and a promise that rejects
    /// is a press that failed.
    on_press: (event: MouseEvent<HTMLButtonElement>) => unknown;
    /// What to show while it works, in place of the label: "saving…". Left
    /// out, the label stays with a spinner in front of it.
    busy_says?: ReactNode;
};

/// A button whose work takes a moment.
///
/// It says it is working from the moment it is pressed until the work is done,
/// takes no second press in between — the second press was the same person,
/// still waiting — and when the work fails it says so on itself instead of
/// going back to looking as if it had never been pressed.
export function Press({ on_press, busy_says, children, disabled = false, title, ...rest }: Props) {
    const [control] = useState(one_press_at_a_time);
    const [busy, set_busy] = useState(false);
    const [failed, set_failed] = useState<string | null>(null);

    const press = (event: MouseEvent<HTMLButtonElement>) => {
        const started = control.press(() => on_press(event));
        if (!started) {
            return;
        }

        set_busy(true);
        set_failed(null);
        started.then(
            () => set_busy(false),
            (cause: unknown) => {
                set_busy(false);
                set_failed(cause instanceof Error ? cause.message : String(cause));
                console.error(cause);
                window.setTimeout(() => set_failed(null), FAILED_FOR);
            },
        );
    };

    return (
        <button
            type="button"
            {...rest}
            disabled={disabled || busy}
            aria-busy={busy || undefined}
            data-failed={failed ? "true" : undefined}
            title={failed ? `that did not work: ${failed}` : title}
            onClick={press}
        >
            {busy ? (
                <span className="inline-flex items-center gap-1">
                    <Spinner />
                    {busy_says ?? children}
                </span>
            ) : (
                children
            )}
        </button>
    );
}
