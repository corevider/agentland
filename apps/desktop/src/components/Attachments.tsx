import { useEffect, useRef, useState } from "react";

import { is_image } from "@/lib/attachments";
import { format_bytes } from "@/lib/core";

/// A picture fetched with the token and shown as an object URL.
///
/// An <img> cannot carry a header, so whoever knows how to get the bytes
/// hands over a loader; the URL it makes is let go when the picture is. The
/// picture is fetched once per `id`, not once per loader: the board polls,
/// every poll drew the card again with a fresh loader, and the picture was
/// refetched and its URL revoked every four seconds — caught mid-swap, it
/// was a broken image.
export function Picture({
    id,
    load,
    alt,
    className,
    onClick,
}: {
    id: string;
    load: () => Promise<string>;
    alt: string;
    className?: string;
    onClick?: () => void;
}) {
    const [src, set_src] = useState<string | null>(null);
    const loader = useRef(load);
    loader.current = load;

    useEffect(() => {
        let made: string | null = null;
        let gone = false;

        loader.current()
            .then((url) => {
                if (gone) {
                    URL.revokeObjectURL(url);
                } else {
                    made = url;
                    set_src(url);
                }
            })
            .catch(() => set_src(null));

        return () => {
            gone = true;
            if (made) {
                URL.revokeObjectURL(made);
            }
        };
    }, [id]);

    if (!src) {
        return <div className={`animate-pulse bg-shallow ${className ?? ""}`} />;
    }

    return <img src={src} alt={alt} className={className} onClick={onClick} draggable={false} />;
}

/// One file on a card: a thumbnail if it is a picture, its name if it is not.
export function AttachmentTile({
    id,
    name,
    kind,
    bytes,
    load,
    on_remove,
    on_open,
    pending,
    marked,
}: {
    /// What tells this file from every other on screen, for as long as it is.
    id: string;
    name: string;
    kind: string;
    bytes: number;
    load: () => Promise<string>;
    on_remove?: () => void;
    on_open?: () => void;
    /// Staged in the window and not on the card yet.
    pending?: boolean;
    /// How many marks a person drew on it.
    marked?: number;
}) {
    return (
        <div
            className={`group relative flex w-[112px] flex-col overflow-hidden rounded-md border bg-lagoon-deep ${
                pending ? "border-dashed border-turquoise/70" : "border-reef"
            }`}
            title={`${name} · ${kind} · ${format_bytes(bytes)}`}
        >
            {is_image(kind) ? (
                <Picture
                    id={id}
                    load={load}
                    alt={name}
                    className={`h-[72px] w-full object-cover ${on_open ? "cursor-zoom-in" : ""}`}
                    onClick={on_open}
                />
            ) : (
                <div className="flex h-[72px] w-full items-center justify-center px-2 text-center font-mono text-[10px] uppercase tracking-[0.1em] text-shell">
                    {kind.split("/")[1]?.slice(0, 12) || "file"}
                </div>
            )}

            <div className="truncate px-1.5 py-1 font-mono text-[9px] text-driftwood">{name}</div>

            {marked ? (
                <span
                    className="absolute left-1 top-1 rounded bg-coral px-1 font-mono text-[9px] font-bold text-white"
                    title={`${marked} mark${marked === 1 ? "" : "s"} drawn on it`}
                >
                    ✎ {marked}
                </span>
            ) : null}

            {on_remove ? (
                <button
                    className="absolute right-1 top-1 hidden rounded bg-lagoon-deep/90 px-1 font-mono text-[10px] text-coral group-hover:block"
                    onClick={(event) => {
                        event.stopPropagation();
                        on_remove();
                    }}
                    title="take this off the card"
                >
                    ✕
                </button>
            ) : null}
        </div>
    );
}

/// A picture on its own, over everything, until it is clicked away.
export function Lightbox({
    id,
    load,
    alt,
    on_close,
}: {
    id: string;
    load: () => Promise<string>;
    alt: string;
    on_close: () => void;
}) {
    useEffect(() => {
        const key = (event: KeyboardEvent) => {
            if (event.key === "Escape") {
                on_close();
            }
        };
        window.addEventListener("keydown", key);
        return () => window.removeEventListener("keydown", key);
    }, [on_close]);

    return (
        <div
            className="fixed inset-0 z-[70] flex cursor-zoom-out items-center justify-center bg-lagoon-deep/85 p-6"
            onClick={on_close}
        >
            <Picture
                id={id}
                load={load}
                alt={alt}
                className="max-h-full max-w-full rounded-md border border-reef object-contain"
            />
        </div>
    );
}
