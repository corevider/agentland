import { useEffect, useRef } from "react";

const LINE_HEIGHT = 16;
const PAGE_HEIGHT = 240;

export interface WheelState {
    scroll_left: number;
    scroll_width: number;
    client_width: number;
    delta_x: number;
    delta_y: number;
    delta_mode: number;
}

export function step_from_wheel(delta: number, mode: number): number {
    if (mode === 1) {
        return delta * LINE_HEIGHT;
    }
    if (mode === 2) {
        return delta * PAGE_HEIGHT;
    }
    return delta;
}

/// A strip of tabs scrolls sideways, but a mouse wheel only turns one way.
/// Take the vertical turn and spend it sideways — but only while the strip has
/// somewhere left to go, so at either end the scroll goes back to the page.
export function sideways_step(state: WheelState): number {
    const room = state.scroll_width - state.client_width;
    if (room <= 1) {
        return 0;
    }

    if (Math.abs(state.delta_x) > Math.abs(state.delta_y)) {
        return 0;
    }

    const wanted = step_from_wheel(state.delta_y, state.delta_mode);
    const target = Math.min(room, Math.max(0, state.scroll_left + wanted));
    return target - state.scroll_left;
}

export function use_sideways_wheel<T extends HTMLElement>() {
    const holder = useRef<T>(null);

    useEffect(() => {
        const element = holder.current;
        if (!element) {
            return;
        }

        // React listens for wheel passively at the root, so preventDefault there
        // does nothing; this one has to be its own non-passive listener.
        const turn = (event: WheelEvent) => {
            const step = sideways_step({
                scroll_left: element.scrollLeft,
                scroll_width: element.scrollWidth,
                client_width: element.clientWidth,
                delta_x: event.deltaX,
                delta_y: event.deltaY,
                delta_mode: event.deltaMode,
            });

            if (step === 0) {
                return;
            }

            element.scrollLeft += step;
            event.preventDefault();
        };

        element.addEventListener("wheel", turn, { passive: false });
        return () => element.removeEventListener("wheel", turn);
    }, []);

    return holder;
}
