export interface Box {
    width: number;
    height: number;
}

export interface Spot {
    left: number;
    top: number;
}

export interface Edges {
    left: number;
    right: number;
    top: number;
    bottom: number;
}

const MARGIN = 8;

function clamp(value: number, lowest: number, highest: number): number {
    // A menu taller than the screen has no good top; the top edge beats the
    // bottom, because a list is read downwards.
    return Math.max(lowest, Math.min(value, Math.max(lowest, highest)));
}

/// Where a menu opened at a point should sit.
///
/// It opens down and to the right of the pointer, the way every menu does, and
/// flips rather than slides when there is no room that way — sliding leaves the
/// menu covering the thing that was right-clicked. Whatever happens it stays
/// inside the screen: measured height, not a guess from the number of rows,
/// because a row with a hint wraps and the guess is short every time.
export function place_menu(at: Spot, box: Box, screen: Box, margin = MARGIN): Spot {
    const room_right = screen.width - at.left - margin;
    const room_below = screen.height - at.top - margin;

    const left = box.width <= room_right ? at.left : at.left - box.width;
    const top = box.height <= room_below ? at.top : at.top - box.height;

    return {
        left: clamp(left, margin, screen.width - box.width - margin),
        top: clamp(top, margin, screen.height - box.height - margin),
    };
}

/// Where a submenu should sit beside the row that opened it.
///
/// Beside, not below: a submenu that drops downwards hides its own parent. It
/// takes the right side when there is room and the left when there is not, and
/// its top is pulled up only as far as it must be to keep the last row on
/// screen — so the first row still lines up with the row you are pointing at.
export function place_submenu(row: Edges, box: Box, screen: Box, margin = MARGIN): Spot {
    const fits_right = row.right + box.width + margin <= screen.width;
    const left = fits_right ? row.right : row.left - box.width;

    return {
        left: clamp(left, margin, screen.width - box.width - margin),
        top: clamp(row.top, margin, screen.height - box.height - margin),
    };
}
