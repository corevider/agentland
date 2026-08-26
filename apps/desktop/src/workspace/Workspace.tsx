import { useCallback, useEffect, useRef, type ReactNode } from "react";

import { Panel } from "@/workspace/Panel";
import { PANELS, clamp_fraction, type Layout, type PanelId, type SlotId } from "@/workspace/layout";

interface Props {
    layout: Layout;
    on_layout: (next: Layout) => void;
    render_panel: (id: PanelId, active: boolean) => ReactNode;
    subtitle_for: (id: PanelId) => string | undefined;
}

function PanelPicker({
    slot,
    current,
    on_pick,
}: {
    slot: SlotId;
    current: PanelId | null;
    on_pick: (slot: SlotId, id: PanelId | null) => void;
}) {
    return (
        <select
            className="rounded-lg border border-reef bg-lagoon-deep px-2 py-1 font-mono text-[10px]"
            value={current ?? ""}
            onChange={(event) => on_pick(slot, (event.target.value || null) as PanelId | null)}
        >
            <option value="">empty</option>
            {PANELS.map((panel) => (
                <option key={panel.id} value={panel.id}>
                    {panel.label}
                </option>
            ))}
        </select>
    );
}

export function Workspace({ layout, on_layout, render_panel, subtitle_for }: Props) {
    const frame = useRef<HTMLDivElement>(null);
    const dragging = useRef<"column" | "row" | null>(null);

    const update = useCallback(
        (next: Layout) => {
            on_layout(next);
        },
        [on_layout],
    );

    useEffect(() => {
        const move = (event: PointerEvent) => {
            const bounds = frame.current?.getBoundingClientRect();
            if (!bounds || !dragging.current) {
                return;
            }

            if (dragging.current === "column") {
                update({
                    ...layout,
                    left_fraction: clamp_fraction((event.clientX - bounds.left) / bounds.width),
                });
            } else {
                update({
                    ...layout,
                    bottom_fraction: clamp_fraction((bounds.bottom - event.clientY) / bounds.height),
                });
            }
        };

        const stop = () => {
            dragging.current = null;
            document.body.style.cursor = "";
        };

        window.addEventListener("pointermove", move);
        window.addEventListener("pointerup", stop);

        return () => {
            window.removeEventListener("pointermove", move);
            window.removeEventListener("pointerup", stop);
        };
    }, [layout, update]);

    const pick = useCallback(
        (slot: SlotId, id: PanelId | null) => {
            update({ ...layout, [slot]: id });
        },
        [layout, update],
    );

    const slot = (id: SlotId, panel: PanelId | null) => {
        if (!panel) {
            return (
                <div className="flex h-full w-full min-h-0 min-w-0 flex-1 items-center justify-center rounded-xl border border-dashed border-reef/70">
                    <PanelPicker slot={id} current={null} on_pick={pick} />
                </div>
            );
        }

        const meta = PANELS.find((entry) => entry.id === panel);

        return (
            <Panel
                title={meta?.label ?? panel}
                subtitle={subtitle_for(panel)}
                actions={<PanelPicker slot={id} current={panel} on_pick={pick} />}
            >
                {render_panel(panel, true)}
            </Panel>
        );
    };

    const bottom_height = layout.bottom ? `${layout.bottom_fraction * 100}%` : "0%";

    return (
        <div ref={frame} className="flex min-h-0 min-w-0 flex-1 flex-col gap-2 p-2">
            <div className="flex min-h-0 min-w-0 flex-1 gap-2">
                <div style={{ width: `${layout.left_fraction * 100}%` }} className="flex min-h-0 min-w-0">
                    {slot("left", layout.left)}
                </div>

                <div
                    className="w-1 shrink-0 cursor-col-resize rounded bg-reef/60 hover:bg-turquoise"
                    onPointerDown={() => {
                        dragging.current = "column";
                        document.body.style.cursor = "col-resize";
                    }}
                />

                <div className="flex min-h-0 min-w-0 flex-1">{slot("right", layout.right)}</div>
            </div>

            {layout.bottom ? (
                <>
                    <div
                        className="h-1 shrink-0 cursor-row-resize rounded bg-reef/60 hover:bg-turquoise"
                        onPointerDown={() => {
                            dragging.current = "row";
                            document.body.style.cursor = "row-resize";
                        }}
                    />
                    <div style={{ height: bottom_height }} className="flex min-h-0 min-w-0">
                        {slot("bottom", layout.bottom)}
                    </div>
                </>
            ) : (
                <div className="flex shrink-0 justify-center">
                    <PanelPicker slot="bottom" current={null} on_pick={pick} />
                </div>
            )}
        </div>
    );
}
