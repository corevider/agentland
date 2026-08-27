import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";

import { PanelBoundary } from "@/workspace/Panel";
import {
    PANELS,
    SLOTS,
    clamp_fraction,
    close_panel,
    move_panel,
    type Layout,
    type PanelId,
    type SlotId,
} from "@/workspace/layout";

interface Props {
    layout: Layout;
    on_layout: (next: Layout) => void;
    render_panel: (id: PanelId, active: boolean) => ReactNode;
    subtitle_for: (id: PanelId) => string | undefined;
}

type Divider = { kind: "column" } | { kind: "row"; column: "left" | "right" };

function label_of(panel: PanelId): string {
    return PANELS.find((entry) => entry.id === panel)?.label ?? panel;
}

function AddPanel({ slot, layout, on_layout }: {
    slot: SlotId;
    layout: Layout;
    on_layout: (next: Layout) => void;
}) {
    const [open, set_open] = useState(false);
    const taken = new Set(SLOTS.flatMap((id) => layout.slots[id].panels));
    const available = PANELS.filter((panel) => !taken.has(panel.id));

    if (available.length === 0) {
        return null;
    }

    return (
        <div className="relative">
            <button
                className="rounded px-1.5 py-0.5 font-mono text-[11px] text-shell hover:text-linen"
                title="add a panel here"
                onClick={() => set_open((value) => !value)}
            >
                +
            </button>
            {open ? (
                <div
                    className="absolute right-0 top-full z-20 mt-1 w-52 rounded-lg border border-reef bg-lagoon-deep p-1 shadow-lg"
                    onMouseLeave={() => set_open(false)}
                >
                    {available.map((panel) => (
                        <button
                            key={panel.id}
                            className="block w-full rounded px-2 py-1 text-left hover:bg-lagoon"
                            onClick={() => {
                                on_layout(move_panel(layout, panel.id, slot));
                                set_open(false);
                            }}
                        >
                            <span className="text-xs text-linen">{panel.label}</span>
                            <span className="ml-1 font-mono text-[10px] text-shade">{panel.hint}</span>
                        </button>
                    ))}
                </div>
            ) : null}
        </div>
    );
}

function SlotView({
    id,
    layout,
    on_layout,
    render_panel,
    subtitle_for,
}: Props & { id: SlotId }) {
    const slot = layout.slots[id];
    const [over, set_over] = useState(false);
    const active = slot.panels[slot.active] ?? null;

    const accept = useCallback(
        (event: React.DragEvent) => {
            event.preventDefault();
            set_over(false);
            const panel = event.dataTransfer.getData("text/agentland-panel") as PanelId;
            if (panel) {
                on_layout(move_panel(layout, panel, id));
            }
        },
        [id, layout, on_layout],
    );

    return (
        <section
            className={`flex h-full min-h-0 w-full min-w-0 flex-col overflow-hidden rounded-xl border bg-lagoon ${
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
                    {slot.panels.map((panel, index) => (
                        <div
                            key={panel}
                            draggable
                            onDragStart={(event) => {
                                event.dataTransfer.setData("text/agentland-panel", panel);
                                event.dataTransfer.effectAllowed = "move";
                            }}
                            onClick={() =>
                                on_layout({
                                    ...layout,
                                    slots: { ...layout.slots, [id]: { ...slot, active: index } },
                                })
                            }
                            className={`group flex cursor-pointer items-center gap-1.5 border-b-2 px-2.5 py-1 ${
                                index === slot.active
                                    ? "border-turquoise text-linen"
                                    : "border-transparent text-shell hover:text-linen"
                            }`}
                        >
                            <span className="whitespace-nowrap text-xs">{label_of(panel)}</span>
                            {index === slot.active ? (
                                <span className="whitespace-nowrap font-mono text-[10px] text-shade">
                                    {subtitle_for(panel)}
                                </span>
                            ) : null}
                            <button
                                className="rounded px-0.5 font-mono text-[10px] text-shade opacity-0 hover:text-coral group-hover:opacity-100"
                                title="close"
                                onClick={(event) => {
                                    event.stopPropagation();
                                    on_layout(close_panel(layout, id, panel));
                                }}
                            >
                                ×
                            </button>
                        </div>
                    ))}
                </div>
                <div className="flex shrink-0 items-center gap-0.5">
                    <button
                        className="rounded px-1.5 py-0.5 font-mono text-[11px] text-shell hover:text-linen"
                        title={layout.maximised === id ? "restore the layout" : "fill the window with this panel"}
                        onClick={() =>
                            on_layout({ ...layout, maximised: layout.maximised === id ? null : id })
                        }
                    >
                        {layout.maximised === id ? "▣" : "▢"}
                    </button>
                    <AddPanel slot={id} layout={layout} on_layout={on_layout} />
                </div>
            </header>

            <PanelBoundary label={active ? label_of(active) : id}>
                <div className="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
                    {slot.panels.length === 0 ? (
                        <div className="flex flex-1 items-center justify-center font-mono text-[11px] text-shade">
                            drop a panel here
                        </div>
                    ) : (
                        slot.panels.map((panel) => (
                            <div
                                key={panel}
                                className={`min-h-0 min-w-0 flex-1 flex-col overflow-hidden ${
                                    panel === active ? "flex" : "hidden"
                                }`}
                            >
                                {render_panel(panel, panel === active)}
                            </div>
                        ))
                    )}
                </div>
            </PanelBoundary>
        </section>
    );
}

export function Workspace({ layout, on_layout, render_panel, subtitle_for }: Props) {
    const frame = useRef<HTMLDivElement>(null);
    const dragging = useRef<Divider | null>(null);

    useEffect(() => {
        const move = (event: PointerEvent) => {
            const bounds = frame.current?.getBoundingClientRect();
            const divider = dragging.current;
            if (!bounds || !divider) {
                return;
            }

            if (divider.kind === "column") {
                on_layout({
                    ...layout,
                    column_fraction: clamp_fraction((event.clientX - bounds.left) / bounds.width),
                });
                return;
            }

            const fraction = clamp_fraction((event.clientY - bounds.top) / bounds.height);
            on_layout(
                divider.column === "left"
                    ? { ...layout, left_row_fraction: fraction }
                    : { ...layout, right_row_fraction: fraction },
            );
        };

        const stop = () => {
            dragging.current = null;
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
    }, [layout, on_layout]);

    const column = (
        side: "left" | "right",
        top: SlotId,
        bottom: SlotId,
        fraction: number,
    ) => {
        const has_bottom = layout.slots[bottom].panels.length > 0;
        const props = { layout, on_layout, render_panel, subtitle_for };

        return (
            <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-1.5">
                <div
                    className="flex min-h-0 min-w-0"
                    style={has_bottom ? { height: `${fraction * 100}%` } : { flex: 1 }}
                >
                    <SlotView id={top} {...props} />
                </div>

                {has_bottom ? (
                    <>
                        <div
                            className="h-1 shrink-0 cursor-row-resize rounded bg-reef/60 hover:bg-turquoise"
                            onPointerDown={() => {
                                dragging.current = { kind: "row", column: side };
                                document.body.style.cursor = "row-resize";
                            }}
                        />
                        <div className="flex min-h-0 min-w-0 flex-1">
                            <SlotView id={bottom} {...props} />
                        </div>
                    </>
                ) : (
                    <div className="flex shrink-0">
                        <SlotView id={bottom} {...props} />
                    </div>
                )}
            </div>
        );
    };

    if (layout.maximised) {
        return (
            <div ref={frame} className="flex min-h-0 min-w-0 flex-1 p-2">
                <SlotView
                    id={layout.maximised}
                    layout={layout}
                    on_layout={on_layout}
                    render_panel={render_panel}
                    subtitle_for={subtitle_for}
                />
            </div>
        );
    }

    return (
        <div ref={frame} className="flex min-h-0 min-w-0 flex-1 gap-1.5 p-1.5">
            <div
                style={{ width: `${layout.column_fraction * 100}%` }}
                className="flex min-h-0 min-w-0"
            >
                {column("left", "left_top", "left_bottom", layout.left_row_fraction)}
            </div>

            <div
                className="w-1 shrink-0 cursor-col-resize rounded bg-reef/60 hover:bg-turquoise"
                onPointerDown={() => {
                    dragging.current = { kind: "column" };
                    document.body.style.cursor = "col-resize";
                }}
            />

            <div className="flex min-h-0 min-w-0 flex-1">
                {column("right", "right_top", "right_bottom", layout.right_row_fraction)}
            </div>
        </div>
    );
}
