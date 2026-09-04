import type { Attachment, Mark, MarkKind, Marks } from "@/lib/core";

export const MARK_TOOLS: { kind: MarkKind; label: string; hint: string }[] = [
    { kind: "box", label: "box", hint: "drag a rectangle around it" },
    { kind: "arrow", label: "arrow", hint: "drag from where to look toward what to look at" },
    { kind: "pen", label: "pen", hint: "draw freehand" },
    { kind: "pin", label: "pin", hint: "click to number a spot" },
    { kind: "label", label: "label", hint: "click to put words there" },
];

/// The colour every mark is drawn in: the same one the board uses for
/// something that needs attention.
export const MARK_COLOR = "#e5705f";

/// The name of the marked copy of a picture: the picture's name, with
/// `.marked` before the extension, and always a PNG since that is what a
/// canvas gives back.
export function derived_name(name: string): string {
    const dot = name.lastIndexOf(".");
    const stem = dot > 0 ? name.slice(0, dot) : name;
    return `${stem}.marked.png`;
}

/// A box as two corners, top-left first, whichever way it was dragged.
export function normalized_box(one: [number, number], other: [number, number]): [[number, number], [number, number]] {
    return [
        [Math.min(one[0], other[0]), Math.min(one[1], other[1])],
        [Math.max(one[0], other[0]), Math.max(one[1], other[1])],
    ];
}

/// Where a mark's number badge sits: the top-left of a box, the tip of an
/// arrow, the first point of a stroke, the spot of a pin or a label.
export function badge_point(mark: Mark): [number, number] | null {
    if (mark.points.length === 0) {
        return null;
    }
    if (mark.kind === "box") {
        return normalized_box(mark.points[0], mark.points[1] ?? mark.points[0])[0];
    }
    if (mark.kind === "arrow") {
        return mark.points[1] ?? mark.points[0];
    }
    return mark.points[0];
}

/// Where a mark's number badge is drawn, in canvas pixels, and how big: a
/// pin's badge sits up and to the right of the pin so the spot stays visible.
export function badge_circle(mark: Mark, scale: number): { x: number; y: number; radius: number } | null {
    const spot = badge_point(mark);
    if (!spot) {
        return null;
    }
    const radius = Math.max(9, 11 * scale);
    const x = spot[0] * scale;
    const y = spot[1] * scale;
    return mark.kind === "pin" ? { x: x + radius * 1.4, y: y - radius * 1.4, radius } : { x, y, radius };
}

/// The mark whose number badge is under a point, if any — the last drawn
/// wins where two overlap, since it is the one on top.
export function badge_under(marks: Mark[], scale: number, x: number, y: number): number | null {
    for (let index = marks.length - 1; index >= 0; index -= 1) {
        const circle = badge_circle(marks[index], scale);
        if (circle && Math.hypot(circle.x - x, circle.y - y) <= circle.radius + 2) {
            return index;
        }
    }
    return null;
}

/// Whether a mark is drawn enough to keep: a box or an arrow of some size, a
/// stroke of more than a dot, a pin or a label anywhere.
export function is_worth_keeping(mark: Mark): boolean {
    if (mark.points.length === 0) {
        return false;
    }
    if (mark.kind === "pin" || mark.kind === "label") {
        return true;
    }
    if (mark.kind === "pen") {
        return mark.points.length > 2;
    }
    const [from, to] = [mark.points[0], mark.points[1] ?? mark.points[0]];
    return Math.abs(from[0] - to[0]) + Math.abs(from[1] - to[1]) > 4;
}

/// The attachments to show as files: what people put on the card, not the
/// copies made from them.
export function originals(attachments: Attachment[] | undefined): Attachment[] {
    return (attachments ?? []).filter((held) => !held.derived_from);
}

/// The marked copy of a picture, if one has been made.
export function marked_copy_of(attachments: Attachment[] | undefined, name: string): Attachment | undefined {
    return (attachments ?? []).find((held) => held.derived_from === name);
}

/// Draw the marks onto a canvas, at a scale from picture pixels to canvas
/// pixels. The same routine draws the live overlay while marking and the
/// flattened copy that is saved, so the two cannot disagree.
export function paint(
    ctx: CanvasRenderingContext2D,
    marks: Marks,
    scale: number,
    options: { draft?: Mark | null; numbered?: boolean; selected?: number | null } = {},
): void {
    const stroke = Math.max(2, 3 * scale);
    const font_size = Math.max(11, 14 * scale);
    ctx.save();
    ctx.lineCap = "round";
    ctx.lineJoin = "round";

    const all = options.draft ? [...marks.marks, options.draft] : marks.marks;
    all.forEach((mark, index) => {
        const is_draft = options.draft === mark;
        ctx.strokeStyle = MARK_COLOR;
        ctx.fillStyle = MARK_COLOR;
        ctx.lineWidth = stroke;
        ctx.globalAlpha = is_draft ? 0.7 : 1;
        ctx.setLineDash([]);

        const at = (point: [number, number]): [number, number] => [point[0] * scale, point[1] * scale];

        if (mark.kind === "box" && mark.points.length >= 1) {
            const [a, b] = normalized_box(mark.points[0], mark.points[1] ?? mark.points[0]);
            const [x, y] = at(a);
            const [x2, y2] = at(b);
            ctx.strokeRect(x, y, x2 - x, y2 - y);
        } else if (mark.kind === "arrow" && mark.points.length >= 1) {
            const [x, y] = at(mark.points[0]);
            const [x2, y2] = at(mark.points[1] ?? mark.points[0]);
            ctx.beginPath();
            ctx.moveTo(x, y);
            ctx.lineTo(x2, y2);
            ctx.stroke();
            const angle = Math.atan2(y2 - y, x2 - x);
            const head = Math.max(10, 16 * scale);
            ctx.beginPath();
            ctx.moveTo(x2, y2);
            ctx.lineTo(x2 - head * Math.cos(angle - Math.PI / 6), y2 - head * Math.sin(angle - Math.PI / 6));
            ctx.lineTo(x2 - head * Math.cos(angle + Math.PI / 6), y2 - head * Math.sin(angle + Math.PI / 6));
            ctx.closePath();
            ctx.fill();
        } else if (mark.kind === "pen" && mark.points.length >= 1) {
            ctx.beginPath();
            mark.points.forEach((point, n) => {
                const [x, y] = at(point);
                if (n === 0) {
                    ctx.moveTo(x, y);
                } else {
                    ctx.lineTo(x, y);
                }
            });
            ctx.stroke();
        } else if (mark.kind === "pin" && mark.points.length >= 1) {
            const [x, y] = at(mark.points[0]);
            ctx.beginPath();
            ctx.arc(x, y, Math.max(4, 6 * scale), 0, Math.PI * 2);
            ctx.fill();
        }

        if (options.numbered !== false && !is_draft) {
            const circle = badge_circle(mark, scale);
            if (circle) {
                const { x: bx, y: by, radius } = circle;
                if (options.selected === index) {
                    ctx.beginPath();
                    ctx.arc(bx, by, radius + Math.max(3, 3 * scale), 0, Math.PI * 2);
                    ctx.fillStyle = "#ffffff";
                    ctx.fill();
                }
                ctx.beginPath();
                ctx.arc(bx, by, radius, 0, Math.PI * 2);
                ctx.fillStyle = MARK_COLOR;
                ctx.fill();
                ctx.fillStyle = "#ffffff";
                ctx.font = `bold ${font_size}px sans-serif`;
                ctx.textAlign = "center";
                ctx.textBaseline = "middle";
                ctx.fillText(String(index + 1), bx, by + 0.5);

                const said = mark.text.trim();
                if (said && (mark.kind === "label" || mark.kind === "pin" || mark.kind === "box")) {
                    ctx.font = `${font_size}px sans-serif`;
                    const width = ctx.measureText(said).width + font_size;
                    const edge = marks.width * scale;
                    const rightward = bx + radius + 4 * scale;
                    const tx = rightward + width > edge ? Math.max(0, bx - radius - 4 * scale - width) : rightward;
                    const ty = by - font_size * 0.8;
                    ctx.fillStyle = "rgba(13, 28, 31, 0.85)";
                    ctx.fillRect(tx, ty, width, font_size * 1.6);
                    ctx.fillStyle = "#ffffff";
                    ctx.textAlign = "left";
                    ctx.fillText(said, tx + font_size / 2, by);
                }
            }
        }
    });

    ctx.restore();
}
