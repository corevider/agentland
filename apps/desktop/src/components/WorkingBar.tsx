import { useEffect, useState } from "react";

import { changes_in_flight, watch_changes } from "@/lib/pending";

/// Long enough that a change the core answers at once never lights the bar.
const SHOW_AFTER = 250;

/// A line along the top of the window while a change is on its way to the core.
///
/// Only changes: every panel reads from the core every few seconds, and a bar
/// lit by those would be a bar that is always lit and says nothing. It covers
/// what a button of its own cannot — a menu entry, a key, a drop — so nothing a
/// person does in the window looks as if it did nothing.
export function WorkingBar() {
    const [count, set_count] = useState(changes_in_flight);
    const [shown, set_shown] = useState(false);
    const working = count > 0;

    useEffect(() => watch_changes(set_count), []);

    useEffect(() => {
        if (!working) {
            set_shown(false);
            return;
        }

        const handle = window.setTimeout(() => set_shown(true), SHOW_AFTER);
        return () => window.clearTimeout(handle);
    }, [working]);

    return (
        <div className="pointer-events-none fixed inset-x-0 top-0 z-50 h-0.5 overflow-hidden" role="status" aria-live="polite">
            {shown ? (
                <>
                    <span className="sr-only">
                        working on {count} change{count === 1 ? "" : "s"}
                    </span>
                    <div className="working-sweep h-full w-1/3 bg-turquoise" />
                </>
            ) : null}
        </div>
    );
}
