import { useEffect, useRef } from "react";

export interface Attention {
    hidden: boolean;
    focused: boolean;
}

export const AWAY_FACTOR = 4;

/// How long to wait before asking the core again. A window nobody can see has
/// nothing to show, and a window sitting behind another can afford to be a few
/// seconds out of date.
export function next_delay(base: number, attention: Attention): number {
    if (attention.hidden) {
        return 0;
    }

    return attention.focused ? base : base * AWAY_FACTOR;
}

/// Ask again every `every` milliseconds while the panel is worth updating.
/// Pauses entirely while the window is hidden and slows down while it is not the
/// window being used.
export function use_poll(run: () => void, every: number, enabled = true): void {
    const held = useRef(run);
    held.current = run;

    useEffect(() => {
        if (!enabled) {
            return;
        }

        let handle = 0;
        let stopped = false;

        const wait = () => {
            const delay = next_delay(every, {
                hidden: document.hidden,
                focused: document.hasFocus(),
            });

            handle = window.setTimeout(step, delay === 0 ? every : delay);
        };

        const step = () => {
            if (stopped) {
                return;
            }

            if (!document.hidden) {
                held.current();
            }

            wait();
        };

        held.current();
        wait();

        const wake = () => {
            if (!document.hidden) {
                window.clearTimeout(handle);
                step();
            }
        };

        document.addEventListener("visibilitychange", wake);
        window.addEventListener("focus", wake);

        return () => {
            stopped = true;
            window.clearTimeout(handle);
            document.removeEventListener("visibilitychange", wake);
            window.removeEventListener("focus", wake);
        };
    }, [enabled, every]);
}
