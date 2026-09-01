export interface Attention {
    hidden: boolean;
    showing: boolean;
    focused: boolean;
    interacting: boolean;
    moving: boolean;
}

export const PACE = {
    interacting: 24,
    moving: 24,
    moving_away: 4,
    still: 0,
    away: 0,
};

/// How often the island is worth drawing.
///
/// Measured on this machine, where WebKitGTK paints without the GPU: the app
/// costs 28% of a core with the island on screen and 4.6% without it. Nothing on
/// a resting island moves — only a working agent's lamp and arm — so a resting
/// island asks for no frames at all, and the scene redraws when something about
/// it actually changes.
export function frame_target(attention: Attention): number {
    if (attention.hidden || !attention.showing) {
        return PACE.away;
    }

    if (attention.interacting) {
        return PACE.interacting;
    }

    if (attention.moving) {
        return attention.focused ? PACE.moving : PACE.moving_away;
    }

    return PACE.still;
}

/// The island may spend this much of the main thread and no more.
export const SHARE = 0.25;

/// However expensive a frame is, a moving island still has to look alive.
export const SLOWEST_MS = 250;

/// How long to wait before asking for the next frame.
///
/// A pace is a wish; what a machine can afford is a measurement. Where a frame
/// costs 4 ms the island runs at the rate it asked for; where the webview draws
/// without a GPU and a frame costs 40 ms, asking for 24 fps only saturates a
/// core — so the wait grows with the cost and the island keeps to its share.
export function frame_wait(fps: number, cost_ms: number): number {
    if (fps <= 0) {
        return 0;
    }

    const wanted = 1000 / fps;
    if (cost_ms <= 0) {
        return wanted;
    }

    return Math.min(SLOWEST_MS, Math.max(wanted, cost_ms / SHARE));
}
