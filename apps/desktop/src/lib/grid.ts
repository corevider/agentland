export const MOST_PANES = 8;
export const SMALLEST_TRACK = 0.25;

/// How many panes fit before the grid stops adding more.
///
/// Eight terminals in one panel is already a wall of text; past that the cells
/// are too small to read a TUI in, so the rest wait their turn on another page.
export function page_of<T>(items: T[], page: number, size = MOST_PANES): T[] {
    const pages = Math.max(1, Math.ceil(items.length / size));
    const wanted = Math.min(Math.max(page, 0), pages - 1);
    return items.slice(wanted * size, wanted * size + size);
}

export function page_count(total: number, size = MOST_PANES): number {
    return Math.max(1, Math.ceil(total / size));
}

/// The shape of the grid: as many columns as asked for, and the rows that need.
export function grid_shape(count: number, wanted_columns: number): { columns: number; rows: number } {
    const columns = Math.max(1, Math.min(wanted_columns, Math.max(count, 1)));
    return { columns, rows: Math.max(1, Math.ceil(Math.max(count, 1) / columns)) };
}

/// A row of equal tracks to start from, or the stored one if it still fits.
export function tracks_for(count: number, stored?: number[]): number[] {
    if (stored && stored.length === count && stored.every((value) => value > 0)) {
        return stored;
    }

    return Array.from({ length: Math.max(count, 1) }, () => 1);
}

/// Dragging the gap between two tracks takes from one and gives to the other,
/// so the panel keeps its size and only the pair either side of the gap moves.
export function resize_tracks(tracks: number[], gap: number, share: number): number[] {
    if (gap < 0 || gap + 1 >= tracks.length) {
        return tracks;
    }

    const pair = tracks[gap] + tracks[gap + 1];
    const first = Math.min(Math.max(share * pair, SMALLEST_TRACK), pair - SMALLEST_TRACK);

    const next = [...tracks];
    next[gap] = first;
    next[gap + 1] = pair - first;
    return next;
}

export function to_template(tracks: number[]): string {
    return tracks.map((value) => `${value.toFixed(3)}fr`).join(" ");
}

/// Where the gap after a track sits, as a fraction of the whole grid — which is
/// where its drag handle belongs.
export function edge(tracks: number[], gap: number): number {
    const total = tracks.reduce((sum, value) => sum + value, 0);
    if (total <= 0) {
        return 0;
    }

    const before = tracks.slice(0, gap + 1).reduce((sum, value) => sum + value, 0);
    return before / total;
}

/// A terminal cell wants to be at least this big before the text in it is worth
/// reading: about eighty columns and a dozen rows at the app's type size.
export const READABLE_CELL = { width: 360, height: 220 };

/// Counting how many fit is a softer question than arranging them: a cell a
/// little under the comfortable size is still worth showing.
export const SMALLEST_CELL = { width: 280, height: 150 };

/// How many columns to use when nobody has said.
///
/// Neither "always two" nor "always four" survives contact with a real layout:
/// two columns in a short panel leave one line of text per terminal, four in a
/// narrow one leave eleven characters. This picks the arrangement whose cells
/// come closest to being readable in the space the panel actually has.
export function best_columns(count: number, width: number, height: number, most = 4): number {
    if (count <= 1 || width <= 0 || height <= 0) {
        return 1;
    }

    let best = 1;
    let best_score = -Infinity;

    for (let columns = 1; columns <= Math.min(most, count); columns += 1) {
        const rows = Math.ceil(count / columns);
        const score = Math.min(
            width / columns / READABLE_CELL.width,
            height / rows / READABLE_CELL.height,
        );

        // A tie goes to fewer columns, which keeps terminals wider.
        if (score > best_score + 0.001) {
            best_score = score;
            best = columns;
        }
    }

    return best;
}

/// How many terminals this much space can show and still be read.
///
/// Eight is the ceiling, but a panel one sixth of the window cannot honour it:
/// eight cells in a strip 400 px wide leave eleven characters of text each. So
/// the panel shows what fits and the rest wait a page away.
export function fits_readably(width: number, height: number, most = MOST_PANES): number {
    if (width <= 0 || height <= 0) {
        return most;
    }

    const across = Math.max(1, Math.floor(width / SMALLEST_CELL.width));
    const down = Math.max(1, Math.floor(height / SMALLEST_CELL.height));
    return Math.max(1, Math.min(most, across * down));
}
