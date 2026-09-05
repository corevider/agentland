import { useCallback, useEffect, useRef, useState } from "react";
import { AnimatePresence, LayoutGroup, motion } from "motion/react";

import type { MenuItem } from "@/components/ContextMenu";
import { on_a_control } from "@/lib/controls";
import { use_sideways_wheel } from "@/lib/wheel";
import { zone_at, zone_rect, zone_says, type Zone } from "@/workspace/dock";
import { PanelBoundary } from "@/workspace/Panel";
import { PANELS, panel_entry, use_services } from "@/workspace/registry";
import {
    add_panel,
    close_tab,
    dock_tab,
    find_stack,
    is_minimised,
    minimise,
    move_tab,
    restore,
    set_active,
    set_fraction,
    split_stack,
    stacks,
    type Layout,
    type Node,
    type Stack,
    type Tab,
} from "@/workspace/layout";

interface Props {
    layout: Layout;
    on_layout: (next: Layout) => void;
    subtitle_for: (panel: string) => string | undefined;
}

/// A tab in the hand: what it is, where it came from, and where the pointer
/// holds it. The tab is carried the way a card is on the board — by the
/// pointer, with a copy following it — because the webview draws no drag image
/// of its own, and a tab that vanishes while it is dragged is one nobody can aim.
interface Carry {
    instance: string;
    panel: string;
    label: string;
    from: string;
    x: number;
    y: number;
    grab_x: number;
    grab_y: number;
    width: number;
    height: number;
}

interface Aim {
    stack: string;
    zone: Zone;
}

interface Drag {
    carry: Carry | null;
    aim: Aim | null;
    take: (stack_id: string, tab: Tab, label: string, event: React.PointerEvent<HTMLElement>) => void;
}

type ViewProps = Props & { drag: Drag };

function AddMenu({
    stack_id,
    layout,
    on_layout,
}: {
    stack_id: string;
    layout: Layout;
    on_layout: (next: Layout) => void;
}) {
    const [open, set_open] = useState<null | "here" | "row" | "column">(null);

    return (
        <div className="relative flex items-center">
            <button
                className="rounded px-1 font-mono text-[11px] text-shade hover:text-linen"
                title="add a panel to this stack"
                onClick={() => set_open(open === "here" ? null : "here")}
            >
                +
            </button>
            <button
                className="rounded px-1 font-mono text-[11px] text-shade hover:text-linen"
                title="split beside this one"
                onClick={() => set_open(open === "row" ? null : "row")}
            >
                ⊞
            </button>
            <button
                className="rounded px-1 font-mono text-[11px] text-shade hover:text-linen"
                title="split below this one"
                onClick={() => set_open(open === "column" ? null : "column")}
            >
                ⊟
            </button>

            <AnimatePresence>
            {open ? (
                <motion.div
                    initial={{ opacity: 0, y: -4, scale: 0.98 }}
                    animate={{ opacity: 1, y: 0, scale: 1 }}
                    exit={{ opacity: 0, y: -4, scale: 0.98 }}
                    transition={{ duration: 0.12, ease: [0.2, 0, 0, 1] }}
                    className="absolute right-0 top-full z-30 mt-1 w-52 rounded-md border border-reef bg-lagoon-deep p-1 shadow-lg"
                    onMouseLeave={() => set_open(null)}
                >
                    <div className="px-1.5 pb-1 font-mono text-[9px] uppercase tracking-[0.14em] text-shade">
                        {open === "here" ? "add as a tab" : open === "row" ? "split beside" : "split below"}
                    </div>
                    {PANELS.map((panel) => (
                        <button
                            key={panel.id}
                            className="block w-full rounded px-1.5 py-1 text-left hover:bg-lagoon"
                            onClick={() => {
                                on_layout(
                                    open === "here"
                                        ? add_panel(layout, stack_id, panel.id)
                                        : split_stack(layout, stack_id, open, panel.id),
                                );
                                set_open(null);
                            }}
                        >
                            <span className="text-[12px] text-linen">{panel.label}</span>
                            <span className="ml-1 font-mono text-[9px] text-shade">{panel.hint}</span>
                        </button>
                    ))}
                </motion.div>
            ) : null}
            </AnimatePresence>
        </div>
    );
}

function panel_choices(run: (panel: string) => void): MenuItem[] {
    return PANELS.map((panel) => ({ label: panel.label, hint: panel.hint, run: () => run(panel.id) }));
}

function StackView({ stack, layout, on_layout, subtitle_for, drag }: ViewProps & { stack: Stack }) {
    const aimed = drag.carry && drag.aim?.stack === stack.id ? drag.aim.zone : null;
    const strip = use_sideways_wheel<HTMLDivElement>();
    const { open_menu } = use_services();
    const active_tab = stack.tabs[stack.active] ?? null;
    const alone = stacks(layout.root).length === 1;
    const maximised = layout.maximised === stack.id;

    // What the menu offers depends on what was right-clicked: a tab answers for
    // its own panel, the empty strip beside it answers for the stack.
    const stack_items = (): MenuItem[] => [
        {
            label: maximised ? "Restore the layout" : "Fill the window",
            hint: "▢",
            disabled: alone && !maximised,
            run: () => on_layout({ ...layout, maximised: maximised ? null : stack.id }),
        },
        { label: "Fold down to the bar", hint: "–", run: () => on_layout(minimise(layout, stack.id)) },
        { label: "Add a panel here", items: panel_choices((panel) => on_layout(add_panel(layout, stack.id, panel))) },
        {
            label: "Split beside",
            items: panel_choices((panel) => on_layout(split_stack(layout, stack.id, "row", panel))),
        },
        {
            label: "Split below",
            items: panel_choices((panel) => on_layout(split_stack(layout, stack.id, "column", panel))),
        },
    ];

    const tab_items = (tab: { instance: string; panel: string }, index: number): MenuItem[] => [
        {
            label: "Close this panel",
            hint: "×",
            danger: true,
            run: () => on_layout(close_tab(layout, stack.id, tab.instance)),
        },
        {
            label: "Close the other tabs",
            disabled: stack.tabs.length < 2,
            run: () => {
                let next = layout;
                for (const other of stack.tabs) {
                    if (other.instance !== tab.instance) {
                        next = close_tab(next, stack.id, other.instance);
                    }
                }
                on_layout(next);
            },
        },
        {
            label: "Open it beside this one",
            hint: "⊞",
            run: () => on_layout(split_stack(layout, stack.id, "row", tab.panel)),
        },
        {
            label: "Open it below this one",
            hint: "⊟",
            run: () => on_layout(split_stack(layout, stack.id, "column", tab.panel)),
        },
        {
            label: "Show this tab",
            disabled: index === stack.active,
            run: () => on_layout(set_active(layout, stack.id, index)),
        },
        ...stack_items(),
    ];

    return (
        <section
            data-stack={stack.id}
            className={`relative flex h-full min-h-0 w-full min-w-0 flex-col overflow-hidden rounded-md border bg-lagoon transition-colors ${
                aimed ? "border-turquoise" : "border-reef"
            }`}
        >
            <header
                data-chrome
                className="flex shrink-0 items-stretch gap-1 border-b border-reef/70 pr-1"
                onContextMenu={(event) => open_menu(event, "This panel", stack_items())}
            >
                <div
                    ref={strip}
                    className="flex min-w-0 flex-1 items-stretch overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
                >
                    {stack.tabs.map((tab, index) => {
                        const meta = panel_entry(tab.panel);
                        return (
                            <div
                                key={tab.instance}
                                // Taken with the pointer, after a few pixels of
                                // travel, so a click still selects the tab and
                                // a press on its close button is a close.
                                onPointerDown={(event) => {
                                    if (event.button !== 0 || on_a_control(event.target)) {
                                        return;
                                    }

                                    const from = { x: event.clientX, y: event.clientY };
                                    const target = event.currentTarget;
                                    const label = meta?.label ?? tab.panel;

                                    const watch = (moved: PointerEvent) => {
                                        if (
                                            Math.abs(moved.clientX - from.x) + Math.abs(moved.clientY - from.y) >
                                            4
                                        ) {
                                            stop();
                                            drag.take(stack.id, tab, label, {
                                                ...event,
                                                clientX: moved.clientX,
                                                clientY: moved.clientY,
                                                currentTarget: target,
                                            } as unknown as React.PointerEvent<HTMLElement>);
                                        }
                                    };

                                    const stop = () => {
                                        window.removeEventListener("pointermove", watch);
                                        window.removeEventListener("pointerup", stop);
                                        window.removeEventListener("pointercancel", stop);
                                        window.removeEventListener("blur", stop);
                                    };

                                    window.addEventListener("pointermove", watch);
                                    window.addEventListener("pointerup", stop);
                                    window.addEventListener("pointercancel", stop);
                                    window.addEventListener("blur", stop);
                                }}
                                onClick={() => on_layout(set_active(layout, stack.id, index))}
                                onContextMenu={(event) =>
                                    open_menu(event, meta?.label ?? tab.panel, tab_items(tab, index))
                                }
                                className={`group relative flex cursor-pointer select-none items-center gap-1.5 px-2.5 py-1 ${
                                    drag.carry?.instance === tab.instance
                                        ? "opacity-30"
                                        : index === stack.active
                                          ? "text-linen"
                                          : "text-shell hover:text-linen"
                                }`}
                            >
                                {index === stack.active ? (
                                    <motion.span
                                        layoutId={`tab-underline-${stack.id}`}
                                        className="absolute inset-x-0 bottom-0 h-[2px] rounded bg-turquoise"
                                        transition={{ type: "spring", stiffness: 520, damping: 40 }}
                                    />
                                ) : null}
                                <span className="whitespace-nowrap text-[12px]">
                                    {meta?.label ?? tab.panel}
                                </span>
                                {index === stack.active ? (
                                    <span className="whitespace-nowrap font-mono text-[10px] text-shade">
                                        {subtitle_for(tab.panel)}
                                    </span>
                                ) : null}
                                <button
                                    className="rounded px-0.5 font-mono text-[10px] text-shade opacity-0 hover:text-coral group-hover:opacity-100"
                                    title="close"
                                    onClick={(event) => {
                                        event.stopPropagation();
                                        on_layout(close_tab(layout, stack.id, tab.instance));
                                    }}
                                >
                                    ×
                                </button>
                            </div>
                        );
                    })}
                </div>

                <div className="flex shrink-0 items-center gap-0.5">
                    <button
                        className="rounded px-1 font-mono text-[13px] leading-none text-shade hover:text-linen"
                        title="fold this panel down to the bar"
                        onClick={() => on_layout(minimise(layout, stack.id))}
                    >
                        –
                    </button>
                    <button
                        className="rounded px-1 font-mono text-[11px] text-shade hover:text-linen"
                        title={layout.maximised === stack.id ? "restore the layout" : "fill the window"}
                        disabled={alone && layout.maximised !== stack.id}
                        onClick={() =>
                            on_layout({
                                ...layout,
                                maximised: layout.maximised === stack.id ? null : stack.id,
                            })
                        }
                    >
                        {layout.maximised === stack.id ? "▣" : "▢"}
                    </button>
                    <AddMenu stack_id={stack.id} layout={layout} on_layout={on_layout} />
                </div>
            </header>

            <PanelBoundary label={active_tab ? (panel_entry(active_tab.panel)?.label ?? active_tab.panel) : stack.id}>
                <div className="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
                    {stack.tabs.length === 0 ? (
                        <div className="flex flex-1 items-center justify-center font-mono text-[11px] text-shade">
                            drop a tab here, or add one with +
                        </div>
                    ) : (
                        stack.tabs.map((tab) => {
                            const meta = panel_entry(tab.panel);
                            if (!meta) {
                                return null;
                            }

                            const showing = tab.instance === active_tab?.instance;

                            // A hidden tab that keeps drawing costs frames for
                            // something nobody is looking at. Only panels that
                            // own live state stay mounted behind their tab.
                            if (!showing && !meta.keep_mounted) {
                                return null;
                            }

                            return (
                                <div
                                    key={tab.instance}
                                    className={`min-h-0 min-w-0 flex-1 flex-col overflow-hidden ${
                                        showing ? "flex" : "hidden"
                                    }`}
                                >
                                    <meta.Component active={showing} instance={tab.instance} />
                                </div>
                            );
                        })
                    )}
                </div>
            </PanelBoundary>

            {aimed && drag.carry ? (
                (() => {
                    const rect = zone_rect(aimed);
                    return (
                        <div
                            className="pointer-events-none absolute z-30 rounded-md border border-turquoise bg-turquoise/10 transition-all duration-150 ease-out"
                            style={{
                                left: `${rect.left * 100}%`,
                                top: `${rect.top * 100}%`,
                                width: `${rect.width * 100}%`,
                                height: `${rect.height * 100}%`,
                            }}
                        >
                            <span className="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 whitespace-nowrap rounded border border-turquoise/60 bg-lagoon-deep px-2 py-0.5 font-mono text-[10px] text-turquoise">
                                {zone_says(aimed, drag.carry.from === stack.id)}
                            </span>
                        </div>
                    );
                })()
            ) : null}
        </section>
    );
}

function folded_away(node: Node, layout: Layout): boolean {
    return node.kind === "stack"
        ? is_minimised(layout, node.id)
        : folded_away(node.first, layout) && folded_away(node.second, layout);
}

function NodeView({ node, layout, on_layout, subtitle_for, drag }: ViewProps & { node: Node }) {
    const { open_menu } = use_services();
    const frame = useRef<HTMLDivElement>(null);
    const dragging = useRef(false);
    const [resizing, set_resizing] = useState(false);

    // Every move rewrites the layout, so the effect below must not depend on it:
    // re-subscribing on each move tore the listeners down mid-drag and the
    // divider let go after a single step. The live values come from refs, and
    // the effect is set up once per divider.
    const latest = useRef({ layout, on_layout, node });
    latest.current = { layout, on_layout, node };

    useEffect(() => {
        const move = (event: PointerEvent) => {
            if (!dragging.current) {
                return;
            }

            const bounds = frame.current?.getBoundingClientRect();
            const held = latest.current.node;
            if (!bounds || held.kind !== "split") {
                return;
            }

            // Without this the browser starts selecting every label the pointer
            // crosses, and the whole window ends up highlighted mid-drag.
            event.preventDefault();

            const fraction =
                held.direction === "row"
                    ? (event.clientX - bounds.left) / bounds.width
                    : (event.clientY - bounds.top) / bounds.height;

            latest.current.on_layout(set_fraction(latest.current.layout, held.id, fraction));
        };

        const stop = () => {
            if (!dragging.current) {
                return;
            }

            dragging.current = false;
            set_resizing(false);
            document.body.style.cursor = "";
            document.body.style.userSelect = "";
        };

        window.addEventListener("pointermove", move, { passive: false });
        window.addEventListener("pointerup", stop);
        window.addEventListener("pointercancel", stop);
        window.addEventListener("blur", stop);

        return () => {
            window.removeEventListener("pointermove", move);
            window.removeEventListener("pointerup", stop);
            window.removeEventListener("pointercancel", stop);
            window.removeEventListener("blur", stop);
        };
    }, []);

    if (node.kind === "stack") {
        return <StackView stack={node} layout={layout} on_layout={on_layout} subtitle_for={subtitle_for} drag={drag} />;
    }

    const first_folded = folded_away(node.first, layout);
    const second_folded = folded_away(node.second, layout);

    // A split whose half is folded gives the whole space to the other half
    // rather than leaving a gap where the panel used to be.
    if (first_folded && !second_folded) {
        return <NodeView node={node.second} layout={layout} on_layout={on_layout} subtitle_for={subtitle_for} drag={drag} />;
    }
    if (second_folded && !first_folded) {
        return <NodeView node={node.first} layout={layout} on_layout={on_layout} subtitle_for={subtitle_for} drag={drag} />;
    }
    if (first_folded && second_folded) {
        return null;
    }

    const row = node.direction === "row";
    const first_size = `${node.fraction * 100}%`;

    return (
        <div
            ref={frame}
            className={`flex min-h-0 min-w-0 flex-1 gap-1.5 ${row ? "flex-row" : "flex-col"}`}
        >
            <div
                className="flex min-h-0 min-w-0"
                style={row ? { width: first_size } : { height: first_size }}
            >
                <NodeView node={node.first} layout={layout} on_layout={on_layout} subtitle_for={subtitle_for} drag={drag} />
            </div>

            <div
                className={`shrink-0 rounded transition-colors ${
                    resizing ? "bg-turquoise" : "bg-reef/60 hover:bg-turquoise"
                } ${row ? "w-1 cursor-col-resize" : "h-1 cursor-row-resize"}`}
                onContextMenu={(event) =>
                    open_menu(event, row ? "This divider" : "This divider", [
                        {
                            label: "Even split",
                            hint: "50/50",
                            run: () => on_layout(set_fraction(layout, node.id, 0.5)),
                        },
                        {
                            label: row ? "Give the left side more" : "Give the top more",
                            hint: "70/30",
                            run: () => on_layout(set_fraction(layout, node.id, 0.7)),
                        },
                        {
                            label: row ? "Give the right side more" : "Give the bottom more",
                            hint: "30/70",
                            run: () => on_layout(set_fraction(layout, node.id, 0.3)),
                        },
                    ])
                }
                onPointerDown={(event) => {
                    event.preventDefault();
                    event.currentTarget.setPointerCapture?.(event.pointerId);
                    dragging.current = true;
                    set_resizing(true);
                    document.body.style.cursor = row ? "col-resize" : "row-resize";
                    document.body.style.userSelect = "none";
                }}
            />

            {resizing ? (
                // A pane can hold an iframe or a terminal canvas, and those swallow
                // pointer moves — the drag would stick the moment the cursor crossed
                // one. This sheet keeps every move coming to the window.
                <div
                    className="fixed inset-0 z-40"
                    style={{ cursor: row ? "col-resize" : "row-resize" }}
                />
            ) : null}

            <div className="flex min-h-0 min-w-0 flex-1">
                <NodeView node={node.second} layout={layout} on_layout={on_layout} subtitle_for={subtitle_for} drag={drag} />
            </div>
        </div>
    );
}

export function Workspace({ layout, on_layout, subtitle_for }: Props) {
    const { open_menu } = use_services();
    const maximised = layout.maximised ? find_stack(layout, layout.maximised) : null;
    const folded = stacks(layout.root).filter((entry) => is_minimised(layout, entry.id));
    const everything_folded = folded.length === stacks(layout.root).length;

    const [carry, set_carry] = useState<Carry | null>(null);
    const [aim, set_aim] = useState<Aim | null>(null);
    const latest = useRef({ layout, on_layout, aim });
    latest.current = { layout, on_layout, aim };

    const take = useCallback((stack_id: string, tab: Tab, label: string, event: React.PointerEvent<HTMLElement>) => {
        const box = event.currentTarget.getBoundingClientRect();
        set_carry({
            instance: tab.instance,
            panel: tab.panel,
            label,
            from: stack_id,
            x: event.clientX,
            y: event.clientY,
            grab_x: event.clientX - box.left,
            grab_y: event.clientY - box.top,
            width: box.width,
            height: box.height,
        });
    }, []);

    // While a tab is in the hand: the copy follows the pointer, the stack under
    // it says where the tab would land, and letting go puts it there. Anything
    // that ends the gesture without a drop — Escape, a cancelled pointer, the
    // window losing focus — puts everything back as it was.
    useEffect(() => {
        if (!carry) {
            return;
        }

        const move = (event: PointerEvent) => {
            set_carry((held) => (held ? { ...held, x: event.clientX, y: event.clientY } : held));
            const under = document.elementFromPoint(event.clientX, event.clientY);
            const holder = under?.closest("[data-stack]") as HTMLElement | null;
            const stack = holder?.getAttribute("data-stack");
            if (!holder || !stack) {
                set_aim(null);
                return;
            }
            const zone = zone_at(event.clientX, event.clientY, holder.getBoundingClientRect());
            set_aim((held) => (held && held.stack === stack && held.zone === zone ? held : { stack, zone }));
        };

        const drop = () => {
            const { layout: now, on_layout: place, aim: at } = latest.current;
            if (at) {
                if (at.zone === "center") {
                    if (at.stack !== carry.from) {
                        place(move_tab(now, carry.instance, at.stack));
                    }
                } else {
                    place(dock_tab(now, carry.instance, at.stack, at.zone));
                }
            }
            set_carry(null);
            set_aim(null);
        };

        const cancel = () => {
            set_carry(null);
            set_aim(null);
        };

        const key = (event: KeyboardEvent) => {
            if (event.key === "Escape") {
                cancel();
            }
        };

        window.addEventListener("pointermove", move);
        window.addEventListener("pointerup", drop);
        window.addEventListener("pointercancel", cancel);
        window.addEventListener("blur", cancel);
        window.addEventListener("keydown", key);
        return () => {
            window.removeEventListener("pointermove", move);
            window.removeEventListener("pointerup", drop);
            window.removeEventListener("pointercancel", cancel);
            window.removeEventListener("blur", cancel);
            window.removeEventListener("keydown", key);
        };
    }, [carry?.instance, carry?.from]);

    const drag: Drag = { carry, aim, take };

    return (
        <LayoutGroup>
        <div className={`flex min-h-0 min-w-0 flex-1 flex-col ${carry ? "select-none" : ""}`}>
            <div className="flex min-h-0 min-w-0 flex-1 p-1.5">
                {maximised ? (
                    <StackView stack={maximised} layout={layout} on_layout={on_layout} subtitle_for={subtitle_for} drag={drag} />
                ) : everything_folded ? (
                    <div className="flex flex-1 items-center justify-center font-mono text-[11px] text-shade">
                        every panel is folded down — pick one from the bar
                    </div>
                ) : (
                    <NodeView node={layout.root} layout={layout} on_layout={on_layout} subtitle_for={subtitle_for} drag={drag} />
                )}
            </div>

            {folded.length > 0 ? (
                <motion.div
                    initial={{ height: 0, opacity: 0 }}
                    animate={{ height: "auto", opacity: 1 }}
                    transition={{ duration: 0.16, ease: [0.2, 0, 0, 1] }}
                    className="flex shrink-0 flex-wrap items-center gap-1 overflow-hidden border-t border-reef/70 px-2 py-1"
                >
                    <span className="font-mono text-[9px] uppercase tracking-[0.14em] text-shade">
                        folded
                    </span>
                    {folded.map((entry) => {
                        const label = entry.tabs
                            .map((tab) => panel_entry(tab.panel)?.label ?? tab.panel)
                            .join(" · ");

                        return (
                            <motion.button
                                key={entry.id}
                                layout
                                initial={{ opacity: 0, y: 6 }}
                                animate={{ opacity: 1, y: 0 }}
                                transition={{ type: "spring", stiffness: 480, damping: 38 }}
                                className="rounded-md border border-reef px-2 py-0.5 text-[11px] text-shell hover:border-turquoise hover:text-linen"
                                title="put it back"
                                onClick={() => on_layout(restore(layout, entry.id))}
                                onContextMenu={(event) =>
                                    open_menu(event, label || "Folded panel", [
                                        { label: "Put it back", hint: "⌃", run: () => on_layout(restore(layout, entry.id)) },
                                        {
                                            label: "Close it for good",
                                            danger: true,
                                            run: () => {
                                                let next = restore(layout, entry.id);
                                                for (const tab of entry.tabs) {
                                                    next = close_tab(next, entry.id, tab.instance);
                                                }
                                                on_layout(next);
                                            },
                                        },
                                    ])
                                }
                            >
                                {label || "empty"} ⌃
                            </motion.button>
                        );
                    })}
                </motion.div>
            ) : null}

            {carry ? (
                <div
                    className="pointer-events-none fixed left-0 top-0 z-50 flex items-center gap-1.5 rounded-md border border-turquoise bg-lagoon-deep px-2.5 py-1 opacity-95 shadow-[0_10px_24px_rgba(0,0,0,0.45)] will-change-transform"
                    style={{
                        width: carry.width,
                        height: carry.height,
                        transform: `translate3d(${carry.x - carry.grab_x}px, ${carry.y - carry.grab_y}px, 0) rotate(2deg)`,
                    }}
                >
                    <span className="whitespace-nowrap text-[12px] text-linen">{carry.label}</span>
                </div>
            ) : null}
        </div>
        </LayoutGroup>
    );
}
