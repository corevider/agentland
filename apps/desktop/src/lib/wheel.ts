import { useEffect, useRef, type RefObject } from "react";

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

/// What sits under the pointer when the wheel turns over a board.
export interface Place {
    over_a_column: boolean;
    column_room: number;
}

/// A column with cards to scroll keeps a vertical turn for itself; the strip
/// only takes the turn from the space around the columns, or from a column
/// that has nothing to scroll and would otherwise swallow it.
export function column_keeps_the_turn(place: Place): boolean {
    return place.over_a_column && place.column_room > 1;
}

/// Where a wheel event landed, read off the board's own markers.
export function place_of(target: EventTarget | null): Place {
    const element = target as Element | null;
    const column = typeof element?.closest === "function" ? element.closest("[data-column]") : null;
    const list = column?.querySelector("[data-cards]") ?? null;

    return {
        over_a_column: column !== null,
        column_room: list ? list.scrollHeight - list.clientHeight : 0,
    };
}

export interface SidewaysOptions {
    /// The element that hears the wheel, when it is wider than the strip that
    /// scrolls: a board's toolbar above its columns moves the columns.
    surface?: RefObject<HTMLElement | null>;
    /// Whether something under the pointer keeps the turn for itself.
    keeps?: (target: EventTarget | null) => boolean;
}

export function use_sideways_wheel<T extends HTMLElement>(options: SidewaysOptions = {}) {
    const holder = useRef<T>(null);
    const { surface, keeps } = options;

    useEffect(() => {
        const element = holder.current;
        const listener = surface?.current ?? element;
        if (!element || !listener) {
            return;
        }

        // React listens for wheel passively at the root, so preventDefault there
        // does nothing; this one has to be its own non-passive listener.
        const turn = (event: WheelEvent) => {
            if (keeps?.(event.target)) {
                return;
            }

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

        listener.addEventListener("wheel", turn, { passive: false });
        return () => listener.removeEventListener("wheel", turn);
    }, [surface, keeps]);

    return holder;
}
