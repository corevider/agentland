import { useEffect, useRef, useState } from "react";

export interface MenuItem {
    label: string;
    hint?: string;
    danger?: boolean;
    disabled?: boolean;
    run: () => void | Promise<void>;
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

    return {
        request,
        open: (event: React.MouseEvent, title: string | undefined, items: MenuItem[]) => {
            event.preventDefault();
            event.stopPropagation();
            set_request({ x: event.clientX, y: event.clientY, title, items });
        },
        close: () => set_request(null),
    };
}

export function ContextMenu({ request, on_close }: { request: MenuRequest | null; on_close: () => void }) {
    const holder = useRef<HTMLDivElement>(null);

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

    const left = Math.min(request.x, window.innerWidth - MENU_WIDTH - 8);
    const top = Math.min(request.y, window.innerHeight - request.items.length * 34 - 48);

    return (
        <div
            ref={holder}
            className="fixed z-50 border border-[#3a4d55] bg-[#141c1f] py-1 shadow-lg"
            style={{ left, top, width: MENU_WIDTH }}
        >
            {request.title ? (
                <div className="border-b border-[#26343a] px-3 py-1 font-mono text-[10px] uppercase tracking-[0.1em] text-[#7b8d94]">
                    {request.title}
                </div>
            ) : null}

            {request.items.map((item, index) => (
                <button
                    key={index}
                    className={`flex w-full items-baseline justify-between gap-3 px-3 py-2 text-left text-xs disabled:opacity-40 ${
                        item.danger ? "text-[#d46969] hover:bg-[#241416]" : "text-[#e3ebee] hover:bg-[#1b262a]"
                    }`}
                    disabled={item.disabled}
                    onClick={() => {
                        on_close();
                        void item.run();
                    }}
                >
                    <span>{item.label}</span>
                    {item.hint ? (
                        <span className="font-mono text-[10px] text-[#5d6e75]">{item.hint}</span>
                    ) : null}
                </button>
            ))}
        </div>
    );
}
