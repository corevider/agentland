import { useEffect, useState } from "react";

import { spin_frame } from "@/lib/spin";

const A_FRAME = 90;

function asked_for_stillness(): boolean {
    return typeof window !== "undefined" && (window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false);
}

/// Something turning, for the seconds a panel is waiting on another machine.
///
/// Somebody who has asked their computer to stop moving still needs to know
/// that something is happening, so the answer to reduced motion is a still
/// glyph rather than an empty space.
///
/// A spinner with words beside it is decoration and says nothing a reader has
/// not already been told, so it only announces itself when it is alone. That is
/// what `label` decides; `Waiting` below is the shape with words.
export function Spinner({ label, className = "" }: { label?: string; className?: string }) {
    const [still] = useState(asked_for_stillness);
    const [tick, set_tick] = useState(0);

    useEffect(() => {
        if (still) {
            return;
        }

        const handle = window.setInterval(() => set_tick((held) => held + 1), A_FRAME);
        return () => window.clearInterval(handle);
    }, [still]);

    const frame = still ? "·" : spin_frame(tick);

    return label ? (
        <span role="status" aria-label={label} className={`font-mono ${className}`}>
            {frame}
        </span>
    ) : (
        <span aria-hidden="true" className={`font-mono ${className}`}>
            {frame}
        </span>
    );
}

/// A wait, said the one way this app says it: something turning, and what it is
/// turning for. Every wait long enough to notice should look like this one.
export function Waiting({ says, className = "" }: { says: string; className?: string }) {
    return (
        <span role="status" className={`inline-flex items-center gap-1.5 ${className}`}>
            <Spinner />
            {says}
        </span>
    );
}
