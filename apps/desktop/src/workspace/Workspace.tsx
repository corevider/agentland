import { useCallback, useEffect, useRef, useState } from "react";
import { AnimatePresence, LayoutGroup, motion } from "motion/react";

import { PanelBoundary } from "@/workspace/Panel";
import { PANELS, panel_entry } from "@/workspace/registry";
import {
    add_panel,
    close_tab,
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
} from "@/workspace/layout";

interface Props {
    layout: Layout;
    on_layout: (next: Layout) => void;
    subtitle_for: (panel: string) => string | undefined;
}

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

function StackView({ stack, layout, on_layout, subtitle_for }: Props & { stack: Stack }) {
    const [over, set_over] = useState(false);
    const active_tab = stack.tabs[stack.active] ?? null;
    const alone = stacks(layout.root).length === 1;

    const accept = useCallback(
        (event: React.DragEvent) => {
            event.preventDefault();
            set_over(false);
            const instance = event.dataTransfer.getData("text/agentland-tab");
            if (instance) {
                on_layout(move_tab(layout, instance, stack.id));
            }
        },
        [layout, on_layout, stack.id],
    );

    return (
        <section
            className={`flex h-full min-h-0 w-full min-w-0 flex-col overflow-hidden rounded-md border bg-lagoon ${
                over ? "border-turquoise" : "border-reef"
            }`}
            onDragOver={(event) => {
                event.preventDefault();
                set_over(true);
            }}
            onDragLeave={() => set_over(false)}
            onDrop={accept}
        >
            <header data-chrome className="flex shrink-0 items-stretch gap-1 border-b border-reef/70 pr-1">
                <div className="flex min-w-0 flex-1 items-stretch overflow-x-auto">
                    {stack.tabs.map((tab, index) => {
                        const meta = panel_entry(tab.panel);
                        return (
                            <div
                                key={tab.instance}
                                draggable
                                onDragStart={(event) => {
                                    event.dataTransfer.setData("text/agentland-tab", tab.instance);
                                    event.dataTransfer.effectAllowed = "move";
                                }}
                                onClick={() => on_layout(set_active(layout, stack.id, index))}
                                className={`group relative flex cursor-pointer items-center gap-1.5 px-2.5 py-1 ${
                                    index === stack.active ? "text-linen" : "text-shell hover:text-linen"
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
        </section>
    );
}

function folded_away(node: Node, layout: Layout): boolean {
    return node.kind === "stack"
        ? is_minimised(layout, node.id)
        : folded_away(node.first, layout) && folded_away(node.second, layout);
}

function NodeView({ node, layout, on_layout, subtitle_for }: Props & { node: Node }) {
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
        return <StackView stack={node} layout={layout} on_layout={on_layout} subtitle_for={subtitle_for} />;
    }

    const first_folded = folded_away(node.first, layout);
    const second_folded = folded_away(node.second, layout);

    // A split whose half is folded gives the whole space to the other half
    // rather than leaving a gap where the panel used to be.
    if (first_folded && !second_folded) {
        return <NodeView node={node.second} layout={layout} on_layout={on_layout} subtitle_for={subtitle_for} />;
    }
    if (second_folded && !first_folded) {
        return <NodeView node={node.first} layout={layout} on_layout={on_layout} subtitle_for={subtitle_for} />;
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
                <NodeView node={node.first} layout={layout} on_layout={on_layout} subtitle_for={subtitle_for} />
            </div>

            <div
                className={`shrink-0 rounded transition-colors ${
                    resizing ? "bg-turquoise" : "bg-reef/60 hover:bg-turquoise"
                } ${row ? "w-1 cursor-col-resize" : "h-1 cursor-row-resize"}`}
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
                <NodeView node={node.second} layout={layout} on_layout={on_layout} subtitle_for={subtitle_for} />
            </div>
        </div>
    );
}

export function Workspace({ layout, on_layout, subtitle_for }: Props) {
    const maximised = layout.maximised ? find_stack(layout, layout.maximised) : null;
    const folded = stacks(layout.root).filter((entry) => is_minimised(layout, entry.id));
    const everything_folded = folded.length === stacks(layout.root).length;

    return (
        <LayoutGroup>
        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
            <div className="flex min-h-0 min-w-0 flex-1 p-1.5">
                {maximised ? (
                    <StackView stack={maximised} layout={layout} on_layout={on_layout} subtitle_for={subtitle_for} />
                ) : everything_folded ? (
                    <div className="flex flex-1 items-center justify-center font-mono text-[11px] text-shade">
                        every panel is folded down — pick one from the bar
                    </div>
                ) : (
                    <NodeView node={layout.root} layout={layout} on_layout={on_layout} subtitle_for={subtitle_for} />
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
                            >
                                {label || "empty"} ⌃
                            </motion.button>
                        );
                    })}
                </motion.div>
            ) : null}
        </div>
        </LayoutGroup>
    );
}
