import { default_layout, type Layout, type Node, type Stack } from "@/workspace/layout";

export interface Preset {
    id: string;
    label: string;
    hint: string;
    build: () => Layout;
}

function counter() {
    let next = 1;
    return () => next++;
}

function stack_of(id: string, panels: string[], take: () => number): Stack {
    return {
        kind: "stack",
        id,
        tabs: panels.map((panel) => ({ panel, instance: `${panel}-${take()}` })),
        active: 0,
    };
}

function build(root: (take: () => number) => Node): Layout {
    const take = counter();
    const tree = root(take);
    return { root: tree, maximised: null, next_id: take() };
}

export const PRESETS: Preset[] = [
    {
        id: "all",
        label: "Everything",
        hint: "every panel at once, for when you want the whole picture",
        build: () =>
            build((take) => ({
                kind: "split",
                id: "s1",
                direction: "row",
                fraction: 0.34,
                first: {
                    kind: "split",
                    id: "s2",
                    direction: "column",
                    fraction: 0.5,
                    first: stack_of("k1", ["island"], take),
                    second: stack_of("k2", ["commander", "dispatch"], take),
                },
                second: {
                    kind: "split",
                    id: "s3",
                    direction: "row",
                    fraction: 0.5,
                    first: {
                        kind: "split",
                        id: "s4",
                        direction: "column",
                        fraction: 0.5,
                        first: stack_of("k3", ["panes"], take),
                        second: stack_of("k4", ["board", "repos"], take),
                    },
                    second: {
                        kind: "split",
                        id: "s5",
                        direction: "column",
                        fraction: 0.5,
                        first: stack_of("k5", ["preview", "crew"], take),
                        second: stack_of("k6", ["memory", "routines", "mail", "approvals", "skills"], take),
                    },
                },
            })),
    },
    {
        id: "crew",
        label: "Crew",
        hint: "the island, the board, and who is working",
        build: () => default_layout(),
    },
    {
        id: "work",
        label: "Work",
        hint: "terminals wide, the board beside them",
        build: () =>
            build((take) => ({
                kind: "split",
                id: "s1",
                direction: "row",
                fraction: 0.31,
                first: {
                    kind: "split",
                    id: "s2",
                    direction: "column",
                    fraction: 0.55,
                    first: stack_of("k1", ["board"], take),
                    second: stack_of("k2", ["island"], take),
                },
                second: stack_of("k3", ["panes"], take),
            })),
    },
    {
        id: "review",
        label: "Review",
        hint: "the diff and the running result, side by side",
        build: () =>
            build((take) => ({
                kind: "split",
                id: "s1",
                direction: "row",
                fraction: 0.46,
                first: {
                    kind: "split",
                    id: "s2",
                    direction: "column",
                    fraction: 0.62,
                    first: stack_of("k1", ["repos"], take),
                    second: stack_of("k2", ["board"], take),
                },
                second: {
                    kind: "split",
                    id: "s3",
                    direction: "column",
                    fraction: 0.5,
                    first: stack_of("k3", ["preview"], take),
                    second: stack_of("k4", ["panes"], take),
                },
            })),
    },
];

export function preset_of(layout: Layout): string | null {
    const shape = (held: Layout) => JSON.stringify(strip(held.root));

    for (const preset of PRESETS) {
        if (shape(preset.build()) === shape(layout)) {
            return preset.id;
        }
    }

    return null;
}

function strip(node: Node): unknown {
    return node.kind === "stack"
        ? { tabs: node.tabs.map((tab) => tab.panel) }
        : { direction: node.direction, first: strip(node.first), second: strip(node.second) };
}
