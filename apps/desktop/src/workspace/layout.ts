export type PanelId = string;

export interface Tab {
    instance: string;
    panel: PanelId;
}

export interface Stack {
    kind: "stack";
    id: string;
    tabs: Tab[];
    active: number;
}

export interface Split {
    kind: "split";
    id: string;
    direction: "row" | "column";
    fraction: number;
    first: Node;
    second: Node;
}

export type Node = Stack | Split;

export interface Layout {
    root: Node;
    maximised: string | null;
    next_id: number;
}

const STORAGE_KEY = "agentland-layout";

export function clamp_fraction(value: number): number {
    return Math.min(0.86, Math.max(0.14, value));
}

function stack(id: string, panels: PanelId[], counter: { next: number }): Stack {
    return {
        kind: "stack",
        id,
        tabs: panels.map((panel) => ({ instance: `${panel}-${counter.next++}`, panel })),
        active: 0,
    };
}

export function default_layout(): Layout {
    const counter = { next: 1 };
    const root: Split = {
        kind: "split",
        id: "s1",
        direction: "row",
        fraction: 0.38,
        first: {
            kind: "split",
            id: "s2",
            direction: "column",
            fraction: 0.58,
            first: stack("k1", ["island"], counter),
            second: stack("k2", ["board"], counter),
        },
        second: stack("k3", ["panes"], counter),
    };

    return { root, maximised: null, next_id: counter.next };
}

export function stacks(node: Node): Stack[] {
    return node.kind === "stack" ? [node] : [...stacks(node.first), ...stacks(node.second)];
}

export function find_stack(layout: Layout, id: string): Stack | null {
    return stacks(layout.root).find((entry) => entry.id === id) ?? null;
}

export function visible_panels(layout: Layout): PanelId[] {
    return stacks(layout.root)
        .map((entry) => entry.tabs[entry.active]?.panel)
        .filter((panel): panel is PanelId => Boolean(panel));
}

export function all_panels(layout: Layout): PanelId[] {
    return stacks(layout.root).flatMap((entry) => entry.tabs.map((tab) => tab.panel));
}

function replace(node: Node, id: string, make: (found: Node) => Node | null): Node | null {
    if (node.id === id) {
        return make(node);
    }

    if (node.kind === "stack") {
        return node;
    }

    const first = replace(node.first, id, make);
    const second = replace(node.second, id, make);

    if (first === node.first && second === node.second) {
        return node;
    }

    if (!first) {
        return second;
    }
    if (!second) {
        return first;
    }

    return { ...node, first, second };
}

function rebuilt(layout: Layout, root: Node | null): Layout {
    const next = root ?? { kind: "stack", id: "k0", tabs: [], active: 0 };
    const live = new Set(stacks(next).map((entry) => entry.id));

    return {
        ...layout,
        root: next,
        maximised: layout.maximised && live.has(layout.maximised) ? layout.maximised : null,
    };
}

export function set_active(layout: Layout, stack_id: string, active: number): Layout {
    return rebuilt(
        layout,
        replace(layout.root, stack_id, (found) =>
            found.kind === "stack" ? { ...found, active } : found,
        ),
    );
}

export function set_fraction(layout: Layout, split_id: string, fraction: number): Layout {
    return rebuilt(
        layout,
        replace(layout.root, split_id, (found) =>
            found.kind === "split" ? { ...found, fraction: clamp_fraction(fraction) } : found,
        ),
    );
}

export function add_panel(layout: Layout, stack_id: string, panel: PanelId): Layout {
    const instance = `${panel}-${layout.next_id}`;
    const root = replace(layout.root, stack_id, (found) =>
        found.kind === "stack"
            ? { ...found, tabs: [...found.tabs, { instance, panel }], active: found.tabs.length }
            : found,
    );

    return { ...rebuilt(layout, root), next_id: layout.next_id + 1 };
}

export function split_stack(
    layout: Layout,
    stack_id: string,
    direction: "row" | "column",
    panel: PanelId,
): Layout {
    const instance = `${panel}-${layout.next_id}`;
    const fresh: Stack = {
        kind: "stack",
        id: `k${layout.next_id}`,
        tabs: [{ instance, panel }],
        active: 0,
    };

    const root = replace(layout.root, stack_id, (found) => ({
        kind: "split",
        id: `s${layout.next_id}`,
        direction,
        fraction: 0.5,
        first: found,
        second: fresh,
    }));

    return { ...rebuilt(layout, root), next_id: layout.next_id + 1 };
}

export function close_tab(layout: Layout, stack_id: string, instance: string): Layout {
    const root = replace(layout.root, stack_id, (found) => {
        if (found.kind !== "stack") {
            return found;
        }

        const tabs = found.tabs.filter((tab) => tab.instance !== instance);
        if (tabs.length === 0 && stacks(layout.root).length > 1) {
            return null;
        }

        return { ...found, tabs, active: Math.min(found.active, Math.max(tabs.length - 1, 0)) };
    });

    return rebuilt(layout, root);
}

export function move_tab(layout: Layout, instance: string, into: string): Layout {
    const source = stacks(layout.root).find((entry) =>
        entry.tabs.some((tab) => tab.instance === instance),
    );
    const tab = source?.tabs.find((entry) => entry.instance === instance);

    if (!source || !tab || source.id === into) {
        return layout;
    }

    const without = replace(layout.root, source.id, (found) => {
        if (found.kind !== "stack") {
            return found;
        }

        const tabs = found.tabs.filter((entry) => entry.instance !== instance);
        return tabs.length === 0
            ? null
            : { ...found, tabs, active: Math.min(found.active, tabs.length - 1) };
    });

    const root = replace(without ?? layout.root, into, (found) =>
        found.kind === "stack"
            ? { ...found, tabs: [...found.tabs, tab], active: found.tabs.length }
            : found,
    );

    return rebuilt(layout, root);
}

export function focus_panel(layout: Layout, panel: PanelId): Layout {
    for (const entry of stacks(layout.root)) {
        const index = entry.tabs.findIndex((tab) => tab.panel === panel);
        if (index >= 0) {
            return set_active(layout, entry.id, index);
        }
    }

    const empty = stacks(layout.root).find((entry) => entry.tabs.length === 0);
    const target = empty ?? stacks(layout.root)[stacks(layout.root).length - 1];

    return target ? add_panel(layout, target.id, panel) : layout;
}

export function upgrade_layout(stored: unknown, known: (panel: string) => boolean): Layout {
    const fresh = default_layout();
    if (typeof stored !== "object" || stored === null) {
        return fresh;
    }

    const held = stored as Record<string, unknown>;

    if (held.root && typeof held.root === "object") {
        const counter = { next: Number(held.next_id) || 1 };
        const clean = prune(held.root as Node, known, counter);
        return clean
            ? {
                  root: clean,
                  maximised: typeof held.maximised === "string" ? held.maximised : null,
                  next_id: counter.next,
              }
            : fresh;
    }

    if (held.slots && typeof held.slots === "object") {
        return from_slots(held as { slots: Record<string, { panels?: string[] }> }, known);
    }

    return fresh;
}

function prune(node: Node, known: (panel: string) => boolean, counter: { next: number }): Node | null {
    if (!node || typeof node !== "object") {
        return null;
    }

    if (node.kind === "stack") {
        const tabs = (node.tabs ?? [])
            .filter((tab) => tab && known(tab.panel))
            .map((tab) => ({
                panel: tab.panel,
                instance: tab.instance || `${tab.panel}-${counter.next++}`,
            }));

        return {
            kind: "stack",
            id: node.id || `k${counter.next++}`,
            tabs,
            active: Math.min(Math.max(node.active ?? 0, 0), Math.max(tabs.length - 1, 0)),
        };
    }

    const first = prune(node.first, known, counter);
    const second = prune(node.second, known, counter);

    if (!first) {
        return second;
    }
    if (!second) {
        return first;
    }

    return {
        kind: "split",
        id: node.id || `s${counter.next++}`,
        direction: node.direction === "column" ? "column" : "row",
        fraction: clamp_fraction(Number(node.fraction) || 0.5),
        first,
        second,
    };
}

function from_slots(
    held: { slots: Record<string, { panels?: string[] }> },
    known: (panel: string) => boolean,
): Layout {
    const counter = { next: 1 };
    const pick = (name: string) => (held.slots[name]?.panels ?? []).filter(known);

    const root: Split = {
        kind: "split",
        id: "s1",
        direction: "row",
        fraction: 0.38,
        first: {
            kind: "split",
            id: "s2",
            direction: "column",
            fraction: 0.58,
            first: stack("k1", pick("left_top"), counter),
            second: stack("k2", pick("left_bottom"), counter),
        },
        second: {
            kind: "split",
            id: "s3",
            direction: "column",
            fraction: 0.62,
            first: stack("k3", pick("right_top"), counter),
            second: stack("k4", pick("right_bottom"), counter),
        },
    };

    const pruned = prune(root, known, counter);
    return { root: pruned ?? default_layout().root, maximised: null, next_id: counter.next };
}

export function load_layout(known: (panel: string) => boolean): Layout {
    try {
        const raw = localStorage.getItem(STORAGE_KEY);
        return raw ? upgrade_layout(JSON.parse(raw), known) : default_layout();
    } catch {
        return default_layout();
    }
}

export function save_layout(layout: Layout): void {
    try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(layout));
    } catch {
        // a layout that cannot persist is still a usable layout
    }
}
