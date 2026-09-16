export interface Point {
    x: number;
    y: number;
}

interface Strip {
    left: number;
    right: number;
    top: number;
    bottom: number;
    scroll_left: number;
    scroll_width: number;
    client_width: number;
}

/// Pixels for this frame: ease up near either visible edge, never past the end.
export function edge_scroll_step(strip: Strip, point: Point, elapsed: number): number {
    const { left, right, top, bottom } = strip;
    if (right <= left || bottom <= top || point.x < left || point.x > right || point.y < top || point.y > bottom) {
        return 0;
    }
    const edge = Math.min(72, (right - left) / 3);
    const speed = point.x < left + edge
        ? -(1 - (point.x - left) / edge)
        : point.x > right - edge
          ? 1 - (right - point.x) / edge
          : 0;
    const room = Math.max(0, strip.scroll_width - strip.client_width);
    const delta = speed * 720 * Math.min(50, Math.max(0, elapsed)) / 1000;
    return Math.min(room, Math.max(0, strip.scroll_left + delta)) - strip.scroll_left;
}

/// A stationary pointer must keep scrolling and aiming at newly revealed columns.
export function follow_board_edge(
    element: HTMLElement,
    pointer: () => Point,
    on_scrolled: (point: Point) => void,
): () => void {
    let frame: number;
    let previous: number | null = null;
    let stopped = false;
    const tick = (now: number) => {
        if (stopped) return;
        const point = pointer();
        const box = element.getBoundingClientRect();
        const step = edge_scroll_step({
            left: box.left,
            right: box.right,
            top: box.top,
            bottom: box.bottom,
            scroll_left: element.scrollLeft,
            scroll_width: element.scrollWidth,
            client_width: element.clientWidth,
        }, point, previous === null ? 0 : now - previous);
        previous = now;
        if (step !== 0) {
            element.scrollLeft += step;
            on_scrolled(point);
        }
        if (!stopped) frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => {
        stopped = true;
        cancelAnimationFrame(frame);
    };
}
