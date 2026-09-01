export interface IslandFrames {
    rendered: number;
    worst_ms: number;
    last_ms: number;
    /// What a frame costs to draw, averaged. On a machine whose webview never
    /// gets a GPU this is tens of milliseconds; with one it is a few.
    cost_ms: number;
    asked_at: number;
}

export const island_frames: IslandFrames = {
    rendered: 0,
    worst_ms: 0,
    last_ms: 0,
    cost_ms: 0,
    asked_at: 0,
};

const SAMPLES = 16;
let recent: number[] = [];

export function median(values: number[]): number {
    if (values.length === 0) {
        return 0;
    }

    const sorted = [...values].sort((first, second) => first - second);
    const middle = Math.floor(sorted.length / 2);
    return sorted.length % 2 === 0 ? (sorted[middle - 1] + sorted[middle]) / 2 : sorted[middle];
}

export function note_island_request(now: number): void {
    if (island_frames.asked_at === 0) {
        island_frames.asked_at = now;
    }
}

export function note_island_frame(now: number): void {
    if (island_frames.last_ms > 0) {
        const gap = now - island_frames.last_ms;
        if (gap < 2000 && gap > island_frames.worst_ms) {
            island_frames.worst_ms = gap;
        }
    }

    if (island_frames.asked_at > 0) {
        // The first frame after a load carries shader compilation and costs ten
        // times the rest, and an occasional stall costs more still. The median
        // of recent frames is what this machine can actually draw.
        recent.push(now - island_frames.asked_at);
        if (recent.length > SAMPLES) {
            recent.shift();
        }

        island_frames.asked_at = 0;
        island_frames.cost_ms = median(recent);
    }

    island_frames.last_ms = now;
    island_frames.rendered += 1;
}

export function reset_island_frames(): void {
    island_frames.rendered = 0;
    island_frames.worst_ms = 0;
    island_frames.last_ms = 0;
    island_frames.cost_ms = 0;
    island_frames.asked_at = 0;
    recent = [];
}
