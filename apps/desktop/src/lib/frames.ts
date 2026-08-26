export interface IslandFrames {
    rendered: number;
    worst_ms: number;
    last_ms: number;
}

export const island_frames: IslandFrames = { rendered: 0, worst_ms: 0, last_ms: 0 };

export function note_island_frame(now: number): void {
    if (island_frames.last_ms > 0) {
        const gap = now - island_frames.last_ms;
        if (gap < 2000 && gap > island_frames.worst_ms) {
            island_frames.worst_ms = gap;
        }
    }

    island_frames.last_ms = now;
    island_frames.rendered += 1;
}

export function reset_island_frames(): void {
    island_frames.rendered = 0;
    island_frames.worst_ms = 0;
    island_frames.last_ms = 0;
}
