/// Where a dragged tab would land on a stack it is held over.
///
/// A stack has five places: its middle takes the tab as one of its own; each
/// edge splits it and gives the tab the half on that side. The edge bands are
/// a quarter of the stack, held between a finger's width and a hand's, so a
/// small stack still has a middle and a big one does not need the pointer at
/// the very edge.
export type Zone = "center" | "left" | "right" | "top" | "bottom";

export interface Box {
    left: number;
    top: number;
    width: number;
    height: number;
}

const BAND_LEAST = 32;
const BAND_MOST = 140;

function band(extent: number): number {
    return Math.min(BAND_MOST, Math.max(BAND_LEAST, extent * 0.25));
}

export function zone_at(x: number, y: number, box: Box): Zone {
    const dx = Math.min(x - box.left, box.left + box.width - x);
    const dy = Math.min(y - box.top, box.top + box.height - y);
    const across = band(box.width);
    const down = band(box.height);

    if (dx >= across && dy >= down) {
        return "center";
    }

    if (dx / across <= dy / down) {
        return x - box.left < box.width / 2 ? "left" : "right";
    }

    return y - box.top < box.height / 2 ? "top" : "bottom";
}

/// The part of the stack the tab would take, as fractions of the stack.
export function zone_rect(zone: Zone): { left: number; top: number; width: number; height: number } {
    switch (zone) {
        case "left":
            return { left: 0, top: 0, width: 0.5, height: 1 };
        case "right":
            return { left: 0.5, top: 0, width: 0.5, height: 1 };
        case "top":
            return { left: 0, top: 0, width: 1, height: 0.5 };
        case "bottom":
            return { left: 0, top: 0.5, width: 1, height: 0.5 };
        default:
            return { left: 0.08, top: 0.08, width: 0.84, height: 0.84 };
    }
}

export function zone_says(zone: Zone, same_stack: boolean): string {
    if (zone === "center") {
        return same_stack ? "already here" : "add as a tab";
    }

    return zone === "left" || zone === "right" ? "split beside" : "split below";
}
