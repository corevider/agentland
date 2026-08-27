import type { Layout } from "@/workspace/layout";

export function bench_layout(with_island: boolean): Layout {
    return {
        root: {
            kind: "split",
            id: "s1",
            direction: "row",
            fraction: with_island ? 0.38 : 0.2,
            first: {
                kind: "stack",
                id: "k1",
                tabs: [
                    with_island
                        ? { panel: "island", instance: "island-1" }
                        : { panel: "board", instance: "board-1" },
                ],
                active: 0,
            },
            second: {
                kind: "stack",
                id: "k2",
                tabs: [{ panel: "panes", instance: "panes-2" }],
                active: 0,
            },
        },
        maximised: null,
        next_id: 3,
    };
}
