/// The order the crew's terminals sit in.
///
/// The core lists sessions in the order they were started, which is not the
/// order anyone wants to read them in. A workspace keeps its own order; anything
/// the order has not heard of yet — a terminal opened a moment ago — goes to the
/// end rather than jumping into the middle.
export function apply_order<T extends { id: string }>(items: T[], order: string[]): T[] {
    const held = new Map(items.map((item) => [item.id, item]));
    const arranged: T[] = [];

    for (const id of order) {
        const item = held.get(id);
        if (item) {
            arranged.push(item);
            held.delete(id);
        }
    }

    for (const item of items) {
        if (held.has(item.id)) {
            arranged.push(item);
        }
    }

    return arranged;
}

/// Put one terminal in another's place.
///
/// Which side of the target it lands on depends on where it came from: dragging
/// down means "put it after that one", dragging up means "put it before". Always
/// inserting before the target moves a pane one place short of where it was
/// dropped, and nothing can ever reach the end of the grid.
export function move_onto(order: string[], moved: string, target: string): string[] {
    if (moved === target) {
        return order;
    }

    const from = order.indexOf(moved);
    const to = order.indexOf(target);

    if (from < 0 || to < 0) {
        return order;
    }

    const without = order.filter((id) => id !== moved);
    const at = without.indexOf(target);
    const insert = from < to ? at + 1 : at;

    return [...without.slice(0, insert), moved, ...without.slice(insert)];
}

/// The order of what is on screen now, so a fresh arrangement can be recorded
/// without inventing an order for terminals nobody has moved.
export function order_of<T extends { id: string }>(items: T[]): string[] {
    return items.map((item) => item.id);
}

/// Forget terminals that have closed, so the stored order does not grow forever.
export function prune_order(order: string[], alive: string[]): string[] {
    const live = new Set(alive);
    return order.filter((id) => live.has(id));
}
