import { useCallback, useEffect, useRef, useState } from "react";

import { PanelBoundary } from "@/workspace/Panel";
import { PANELS, panel_entry } from "@/workspace/registry";
import {
    add_panel,
    close_tab,
    find_stack,
    move_tab,
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

            {open ? (
                <div
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
                </div>
            ) : null}
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
            <header className="flex shrink-0 items-stretch gap-1 border-b border-reef/70 pr-1">
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
                                className={`group flex cursor-pointer items-center gap-1.5 border-b-2 px-2.5 py-1 ${
                                    index === stack.active
                                        ? "border-turquoise text-linen"
                                        : "border-transparent text-shell hover:text-linen"
                                }`}
                            >
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

function NodeView({ node, layout, on_layout, subtitle_for }: Props & { node: Node }) {
    const frame = useRef<HTMLDivElement>(null);
    const dragging = useRef(false);

    useEffect(() => {
        if (node.kind !== "split") {
            return;
        }

        const move = (event: PointerEvent) => {
            const bounds = frame.current?.getBoundingClientRect();
            if (!bounds || !dragging.current) {
                return;
            }

            const fraction =
                node.direction === "row"
                    ? (event.clientX - bounds.left) / bounds.width
                    : (event.clientY - bounds.top) / bounds.height;

            on_layout(set_fraction(layout, node.id, fraction));
        };

        const stop = () => {
            dragging.current = false;
            document.body.style.cursor = "";
        };

        window.addEventListener("pointermove", move);
        window.addEventListener("pointerup", stop);
        window.addEventListener("pointercancel", stop);

        return () => {
            window.removeEventListener("pointermove", move);
            window.removeEventListener("pointerup", stop);
            window.removeEventListener("pointercancel", stop);
        };
    }, [layout, node, on_layout]);

    if (node.kind === "stack") {
        return <StackView stack={node} layout={layout} on_layout={on_layout} subtitle_for={subtitle_for} />;
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
                className={`shrink-0 rounded bg-reef/60 hover:bg-turquoise ${
                    row ? "w-1 cursor-col-resize" : "h-1 cursor-row-resize"
                }`}
                onPointerDown={() => {
                    dragging.current = true;
                    document.body.style.cursor = row ? "col-resize" : "row-resize";
                }}
            />

            <div className="flex min-h-0 min-w-0 flex-1">
                <NodeView node={node.second} layout={layout} on_layout={on_layout} subtitle_for={subtitle_for} />
            </div>
        </div>
    );
}

export function Workspace({ layout, on_layout, subtitle_for }: Props) {
    const maximised = layout.maximised ? find_stack(layout, layout.maximised) : null;

    return (
        <div className="flex min-h-0 min-w-0 flex-1 p-1.5">
            {maximised ? (
                <StackView stack={maximised} layout={layout} on_layout={on_layout} subtitle_for={subtitle_for} />
            ) : (
                <NodeView node={layout.root} layout={layout} on_layout={on_layout} subtitle_for={subtitle_for} />
            )}
        </div>
    );
}
