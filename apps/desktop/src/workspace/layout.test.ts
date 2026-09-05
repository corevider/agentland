import { describe, expect, it } from "vitest";

import {
    add_panel,
    is_minimised,
    minimise,
    restore,
    visible_stacks,
    close_tab,
    default_layout,
    focus_panel,
    move_tab,
    set_fraction,
    split_stack,
    stacks,
    upgrade_layout,
    visible_panels,
    type Layout, dock_tab, type Node, type Split } from "@/workspace/layout";

const known = (panel: string) =>
    ["island", "panes", "board", "preview", "repos", "crew", "skills"].includes(panel);

function tabs_of(layout: Layout): string[][] {
    return stacks(layout.root).map((stack) => stack.tabs.map((tab) => tab.panel));
}

describe("a stack can be split in either direction", () => {
    it("puts the new panel beside or below without disturbing the old one", () => {
        const layout = default_layout();
        const first = stacks(layout.root)[0];

        const beside = split_stack(layout, first.id, "row", "skills");
        expect(stacks(beside.root)).toHaveLength(stacks(layout.root).length + 1);
        expect(visible_panels(beside)).toContain("island");
        expect(visible_panels(beside)).toContain("skills");

        const below = split_stack(beside, first.id, "column", "repos");
        expect(stacks(below.root)).toHaveLength(stacks(layout.root).length + 2);
        expect(visible_panels(below)).toContain("repos");
    });

    it("keeps splitting, with no ceiling on depth", () => {
        let layout = default_layout();
        for (let step = 0; step < 12; step += 1) {
            const target = stacks(layout.root)[0];
            layout = split_stack(layout, target.id, step % 2 === 0 ? "row" : "column", "crew");
        }

        expect(stacks(layout.root)).toHaveLength(3 + 12);
    });
});

describe("the same panel can be open more than once", () => {
    it("gives every copy its own instance", () => {
        const layout = default_layout();
        const [first, second] = stacks(layout.root);

        const twice = add_panel(add_panel(layout, first.id, "preview"), second.id, "preview");
        const instances = stacks(twice.root)
            .flatMap((stack) => stack.tabs)
            .filter((tab) => tab.panel === "preview")
            .map((tab) => tab.instance);

        expect(instances).toHaveLength(2);
        expect(new Set(instances).size).toBe(2);
    });

    it("closing one copy leaves the other", () => {
        const layout = default_layout();
        const [first, second] = stacks(layout.root);
        const twice = add_panel(add_panel(layout, first.id, "preview"), second.id, "preview");

        const doomed = stacks(twice.root)
            .flatMap((stack) => stack.tabs)
            .find((tab) => tab.panel === "preview");

        const after = close_tab(twice, first.id, doomed!.instance);
        const left = stacks(after.root)
            .flatMap((stack) => stack.tabs)
            .filter((tab) => tab.panel === "preview");

        expect(left).toHaveLength(1);
    });
});

describe("moving a tab", () => {
    it("carries it to another stack and leaves nothing behind", () => {
        const layout = default_layout();
        const [source, , target] = stacks(layout.root);
        const tab = source.tabs[0];

        const moved = move_tab(layout, tab.instance, target.id);
        const holder = stacks(moved.root).find((stack) =>
            stack.tabs.some((entry) => entry.instance === tab.instance),
        );

        expect(holder?.id).toBe(target.id);
        expect(tabs_of(moved).flat().filter((panel) => panel === tab.panel)).toHaveLength(1);
    });

    it("collapses a stack that its last tab left", () => {
        const layout = default_layout();
        const [source, , target] = stacks(layout.root);

        const moved = move_tab(layout, source.tabs[0].instance, target.id);
        expect(stacks(moved.root).some((stack) => stack.id === source.id)).toBe(false);
    });
});

describe("closing", () => {
    it("keeps the last stack even when it is empty, so there is somewhere to drop", () => {
        let layout = default_layout();
        for (const stack of stacks(layout.root).slice(1)) {
            for (const tab of stack.tabs) {
                layout = close_tab(layout, stack.id, tab.instance);
            }
        }

        const last = stacks(layout.root);
        expect(last).toHaveLength(1);

        for (const tab of [...last[0].tabs]) {
            layout = close_tab(layout, last[0].id, tab.instance);
        }

        expect(stacks(layout.root)).toHaveLength(1);
        expect(visible_panels(layout)).toHaveLength(0);
    });

    it("drops a maximised stack that no longer exists", () => {
        const layout = default_layout();
        const [first, , target] = stacks(layout.root);
        const maximised = { ...layout, maximised: first.id };

        const moved = move_tab(maximised, first.tabs[0].instance, target.id);
        expect(moved.maximised).toBeNull();
    });
});

describe("what is restored from storage", () => {
    it("upgrades the four-slot layout the previous version saved", () => {
        const upgraded = upgrade_layout(
            {
                slots: {
                    left_top: { panels: ["island"], active: 0 },
                    left_bottom: { panels: ["board"], active: 0 },
                    right_top: { panels: ["panes", "skills"], active: 1 },
                    right_bottom: { panels: [], active: 0 },
                },
            },
            known,
        );

        expect(tabs_of(upgraded).flat()).toEqual(["island", "board", "panes", "skills"]);
    });

    it("drops a panel that no longer exists", () => {
        const upgraded = upgrade_layout(
            {
                root: {
                    kind: "stack",
                    id: "k1",
                    tabs: [
                        { panel: "island", instance: "island-1" },
                        { panel: "seance", instance: "seance-2" },
                    ],
                    active: 1,
                },
                next_id: 3,
            },
            known,
        );

        expect(tabs_of(upgraded)).toEqual([["island"]]);
        expect(stacks(upgraded.root)[0].active).toBe(0);
    });

    it("falls back to the default when the stored value is nonsense", () => {
        expect(tabs_of(upgrade_layout("not a layout", known))).toEqual(tabs_of(default_layout()));
        expect(tabs_of(upgrade_layout(null, known))).toEqual(tabs_of(default_layout()));
    });
});

describe("focusing a panel", () => {
    it("selects the copy that is already open", () => {
        const layout = add_panel(default_layout(), stacks(default_layout().root)[0].id, "skills");
        const focused = focus_panel(layout, "skills");
        expect(visible_panels(focused)).toContain("skills");
    });

    it("opens one when there is none", () => {
        const layout = default_layout();
        expect(visible_panels(layout)).not.toContain("repos");
        expect(visible_panels(focus_panel(layout, "repos"))).toContain("repos");
    });
});

describe("resizing", () => {
    it("clamps a divider dragged past the edge", () => {
        const layout = default_layout();
        const split = layout.root.kind === "split" ? layout.root : null;

        const squashed = set_fraction(layout, split!.id, 0.01);
        const stretched = set_fraction(layout, split!.id, 4);

        const read = (held: Layout) => (held.root.kind === "split" ? held.root.fraction : null);
        expect(read(squashed)).toBeGreaterThan(0.1);
        expect(read(stretched)).toBeLessThan(0.9);
    });
});

describe("folding a panel down to the bar", () => {
    it("gives its room to the panel beside it and keeps its tabs", () => {
        const layout = default_layout();
        const [first] = stacks(layout.root);

        const folded = minimise(layout, first.id);
        expect(is_minimised(folded, first.id)).toBe(true);
        expect(visible_stacks(folded)).toHaveLength(stacks(layout.root).length - 1);
        expect(stacks(folded.root)).toHaveLength(stacks(layout.root).length);
        expect(visible_panels(folded)).not.toContain("island");

        const back = restore(folded, first.id);
        expect(visible_panels(back)).toContain("island");
    });

    it("folding twice changes nothing", () => {
        const layout = default_layout();
        const [first] = stacks(layout.root);
        const once = minimise(layout, first.id);

        expect(minimise(once, first.id).minimised).toEqual(once.minimised);
    });

    it("a maximised panel that is folded stops being maximised", () => {
        const layout = default_layout();
        const [first] = stacks(layout.root);

        const both = minimise({ ...layout, maximised: first.id }, first.id);
        expect(both.maximised).toBeNull();
    });

    it("every panel can be folded, and the bar is the way back", () => {
        let layout = default_layout();
        for (const stack of stacks(layout.root)) {
            layout = minimise(layout, stack.id);
        }

        expect(visible_stacks(layout)).toHaveLength(0);
        expect(visible_panels(layout)).toHaveLength(0);
        expect(layout.minimised).toHaveLength(stacks(layout.root).length);
    });

    it("a stack that disappears is dropped from the bar rather than haunting it", () => {
        const layout = default_layout();
        const [source, , target] = stacks(layout.root);

        const folded = minimise(layout, source.id);
        const moved = move_tab(folded, source.tabs[0].instance, target.id);

        expect(stacks(moved.root).some((stack) => stack.id === source.id)).toBe(false);
        expect(moved.minimised).not.toContain(source.id);
    });
});

describe("docking a tab beside or below a stack", () => {
    const parent_of = (node: Node, id: string): Split | null => {
        if (node.kind !== "split") {
            return null;
        }
        if (node.first.id === id || node.second.id === id) {
            return node;
        }
        return parent_of(node.first, id) ?? parent_of(node.second, id);
    };

    it("puts the tab in a new stack on the side it was dropped, and leaves nothing behind", () => {
        const layout = default_layout();
        const [island, , panes] = stacks(layout.root);

        const moved = dock_tab(layout, island.tabs[0].instance, panes.id, "right");

        const around = parent_of(moved.root, panes.id);
        expect(around?.direction).toBe("row");
        expect(around?.first.id).toBe(panes.id);
        expect(around?.second.kind === "stack" && around.second.tabs[0]?.panel).toBe("island");
        expect(tabs_of(moved).flat().filter((panel) => panel === "island")).toHaveLength(1);
        expect(stacks(moved.root).map((stack) => stack.id)).not.toContain(island.id);
    });

    it("takes the first half for left and top", () => {
        const layout = default_layout();
        const [, board, panes] = stacks(layout.root);

        const moved = dock_tab(layout, board.tabs[0].instance, panes.id, "top");

        const around = parent_of(moved.root, panes.id);
        expect(around?.direction).toBe("column");
        expect(around?.first.kind === "stack" && around.first.tabs[0]?.panel).toBe("board");
        expect(around?.second.id).toBe(panes.id);
    });

    it("splits a stack off its own tab, but not a stack with only that tab", () => {
        const layout = default_layout();
        const [, , panes] = stacks(layout.root);
        expect(dock_tab(layout, panes.tabs[0].instance, panes.id, "right")).toBe(layout);

        const crowded = add_panel(layout, panes.id, "crew");
        const split = dock_tab(crowded, `crew-${layout.next_id}`, panes.id, "right");
        expect(stacks(split.root)).toHaveLength(4);
        expect(tabs_of(split)).toContainEqual(["crew"]);
        expect(tabs_of(split)).toContainEqual(["panes"]);
    });
});

describe("dropping a tab at a seat in a strip", () => {
    it("lands between the tabs already there and becomes the shown one", () => {
        const layout = default_layout();
        const [island, , panes] = stacks(layout.root);
        const crowded = add_panel(add_panel(layout, panes.id, "crew"), panes.id, "repos");

        const moved = move_tab(crowded, island.tabs[0].instance, panes.id, 1);
        const holder = stacks(moved.root).find((stack) => stack.id === panes.id);

        expect(holder?.tabs.map((tab) => tab.panel)).toEqual(["panes", "island", "crew", "repos"]);
        expect(holder?.active).toBe(1);
    });

    it("reorders within its own strip, counting the seat with the tab still in place", () => {
        const layout = default_layout();
        const [, , panes] = stacks(layout.root);
        const crowded = add_panel(add_panel(layout, panes.id, "crew"), panes.id, "repos");
        const first = panes.tabs[0].instance;

        const moved = move_tab(crowded, first, panes.id, 3);
        const holder = stacks(moved.root).find((stack) => stack.id === panes.id);
        expect(holder?.tabs.map((tab) => tab.panel)).toEqual(["crew", "repos", "panes"]);

        expect(move_tab(crowded, first, panes.id, 0)).toBe(crowded);
        expect(move_tab(crowded, first, panes.id, 1)).toBe(crowded);
    });
});
