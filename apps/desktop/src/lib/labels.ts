export interface Label {
    id: string;
    /// Where the thing it names is, on the canvas. The label hangs above this
    /// point, centred on it.
    x: number;
    y: number;
    width: number;
    height: number;
}

export interface Spot {
    x: number;
    y: number;
}

const GAP = 3;

function overlaps(left: Label & Spot, right: Label & Spot, gap: number): boolean {
    const apart_sideways =
        Math.abs(left.x - right.x) >= (left.width + right.width) / 2 + gap;
    const apart_vertically =
        left.y - left.height >= right.y + gap || right.y - right.height >= left.y + gap;

    return !apart_sideways && !apart_vertically;
}

/// Move labels off each other, keeping each above the thing it names.
///
/// Two agents standing near each other on the island put their name tags in the
/// same place, and the one drawn second wins — which reads as a single unusable
/// smear. So a label that would land on one already placed is lifted just far
/// enough to clear it. Nearest first: the station closest to the camera keeps
/// the spot it earned, and the ones behind it stack upwards, which is the order
/// a person reads depth in anyway.
export function spread_labels(labels: Label[], canvas: { width: number; height: number }, gap = GAP): Map<string, Spot> {
    const nearest_first = [...labels].sort((left, right) => right.y - left.y);
    const placed: Array<Label & Spot> = [];
    const spots = new Map<string, Spot>();

    for (const label of nearest_first) {
        let y = label.y;

        // Lift until it clears everything already standing, or until lifting
        // would push it off the top — a label above the canvas helps nobody, so
        // it stays where it is and the overlap is accepted rather than hidden.
        for (let attempt = 0; attempt < placed.length + 1; attempt += 1) {
            const held = { ...label, x: label.x, y };
            const clash = placed.find((other) => overlaps(held, other, gap));
            if (!clash) {
                break;
            }

            const lifted = clash.y - clash.height - gap;
            if (lifted - label.height < 0) {
                break;
            }

            y = lifted;
        }

        const spot = { x: label.x, y };
        placed.push({ ...label, ...spot });
        spots.set(label.id, spot);
    }

    return spots;
}
