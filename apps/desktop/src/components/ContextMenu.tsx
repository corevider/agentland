import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";

import { place_menu, place_submenu, type Spot } from "@/lib/menu_place";

export interface MenuItem {
    label: string;
    hint?: string;
    danger?: boolean;
    disabled?: boolean;
    items?: MenuItem[];
    run?: () => void | Promise<void>;
}

export interface MenuRequest {
    x: number;
    y: number;
    title?: string;
    items: MenuItem[];
}

const MENU_WIDTH = 232;

export function useContextMenu() {
    const [request, set_request] = useState<MenuRequest | null>(null);

    useEffect(() => {
        const suppress = (event: MouseEvent) => {
            const target = event.target as HTMLElement | null;
            if (target?.closest("[data-native-menu]")) {
                return;
            }
            event.preventDefault();
        };

        document.addEventListener("contextmenu", suppress);
        return () => document.removeEventListener("contextmenu", suppress);
    }, []);

    // Stable, because the workspace hands these to every panel through a
    // memoised service object: a fresh function each render would rebuild it.
    const open = useCallback(
        (event: React.MouseEvent, title: string | undefined, items: MenuItem[]) => {
            event.preventDefault();
            event.stopPropagation();
            set_request({ x: event.clientX, y: event.clientY, title, items });
        },
        [],
    );

    const close = useCallback(() => set_request(null), []);

    return { request, open, close };
}

function Row({ item, on_close }: { item: MenuItem; on_close: () => void }) {
    const [open, set_open] = useState(false);
    const [spot, set_spot] = useState<Spot | null>(null);
    const row = useRef<HTMLDivElement>(null);
    const panel = useRef<HTMLDivElement>(null);
    const nested = item.items ?? [];

    // Measured, not guessed: the submenu is laid out once, read, and then put
    // where it fits. It is invisible for that one frame rather than flashing in
    // the wrong place first.
    useLayoutEffect(() => {
        if (!open) {
            set_spot(null);
            return;
        }

        const beside = row.current?.getBoundingClientRect();
        const box = panel.current?.getBoundingClientRect();
        if (!beside || !box) {
            return;
        }

        set_spot(
            place_submenu(
                { left: beside.left, right: beside.right, top: beside.top, bottom: beside.bottom },
                { width: box.width, height: box.height },
                { width: window.innerWidth, height: window.innerHeight },
            ),
        );
    }, [open]);

    if (nested.length === 0) {
        return (
            <button
                className={`flex w-full items-baseline justify-between gap-3 px-3 py-2 text-left text-xs disabled:opacity-40 ${
                    item.danger ? "text-coral hover:bg-lagoon" : "text-linen hover:bg-shallow"
                }`}
                disabled={item.disabled}
                onClick={() => {
                    on_close();
                    void item.run?.();
                }}
            >
                <span>{item.label}</span>
                {item.hint ? <span className="font-mono text-[10px] text-shade">{item.hint}</span> : null}
            </button>
        );
    }

    return (
        <div
            ref={row}
            className="relative"
            onMouseEnter={() => set_open(true)}
            onMouseLeave={() => set_open(false)}
        >
            <button
                className="flex w-full items-baseline justify-between gap-3 px-3 py-2 text-left text-xs text-linen hover:bg-shallow disabled:opacity-40"
                disabled={item.disabled}
                onClick={() => set_open((held) => !held)}
            >
                <span>{item.label}</span>
                <span className="font-mono text-[10px] text-shade">›</span>
            </button>

            {open ? (
                <div
                    ref={panel}
                    className={`fixed z-50 max-h-[60vh] overflow-y-auto rounded-lg border border-foam bg-lagoon py-1 shadow-lg ${
                        spot ? "" : "pointer-events-none opacity-0"
                    }`}
                    style={{ width: MENU_WIDTH, left: spot?.left ?? 0, top: spot?.top ?? 0 }}
                >
                    {nested.map((child, index) => (
                        <Row key={index} item={child} on_close={on_close} />
                    ))}
                </div>
            ) : null}
        </div>
    );
}

export function ContextMenu({ request, on_close }: { request: MenuRequest | null; on_close: () => void }) {
    const holder = useRef<HTMLDivElement>(null);
    const [spot, set_spot] = useState<Spot | null>(null);

    // The menu is measured where it lands and then moved, because how tall it
    // is depends on what is in it — a row with a hint wraps, a title adds a
    // line — and a guess from the number of rows was short every time, which is
    // how the last item ended up under the bottom of the window.
    useLayoutEffect(() => {
        if (!request) {
            set_spot(null);
            return;
        }

        const box = holder.current?.getBoundingClientRect();
        if (!box) {
            return;
        }

        set_spot(
            place_menu(
                { left: request.x, top: request.y },
                { width: box.width, height: box.height },
                { width: window.innerWidth, height: window.innerHeight },
            ),
        );
    }, [request]);

    useEffect(() => {
        if (!request) {
            return;
        }

        const dismiss = (event: MouseEvent) => {
            if (!holder.current?.contains(event.target as Node)) {
                on_close();
            }
        };

        const escape = (event: KeyboardEvent) => {
            if (event.key === "Escape") {
                on_close();
            }
        };

        window.addEventListener("mousedown", dismiss);
        window.addEventListener("keydown", escape);
        window.addEventListener("blur", on_close);

        return () => {
            window.removeEventListener("mousedown", dismiss);
            window.removeEventListener("keydown", escape);
            window.removeEventListener("blur", on_close);
        };
    }, [request, on_close]);

    if (!request) {
        return null;
    }

    return (
        <div
            ref={holder}
            className={`fixed z-50 max-h-[80vh] overflow-y-auto rounded-lg border border-foam bg-lagoon py-1 shadow-lg ${
                spot ? "" : "pointer-events-none opacity-0"
            }`}
            style={{ left: spot?.left ?? request.x, top: spot?.top ?? request.y, width: MENU_WIDTH }}
        >
            {request.title ? (
                <div className="border-b border-reef px-3 py-1 font-mono text-[10px] uppercase tracking-[0.1em] text-shell">
                    {request.title}
                </div>
            ) : null}

            {request.items.map((item, index) => (
                <Row key={index} item={item} on_close={on_close} />
            ))}
        </div>
    );
}
