import { useEffect, useRef, useState } from "react";

import { use_poll } from "@/lib/poll";

export interface Screen {
    buffer: {
        active: {
            length: number;
            getLine: (index: number) => { translateToString: (trim?: boolean) => string } | undefined;
        };
    };
}

/// What a pty writes is not a record of what was said — it is a stream of screen
/// redraws, positioned by cursor moves. A live pane proved it: read line by line
/// the spinner's letters land in the text and a sentence comes out as "✢ U a".
/// So reading it needs a terminal, and the app already ships one.
export function lines_from_screen(screen: Screen): string[] {
    const lines: string[] = [];

    for (let index = 0; index < screen.buffer.active.length; index += 1) {
        lines.push(screen.buffer.active.getLine(index)?.translateToString(true) ?? "");
    }

    return lines;
}

/// The screen still holds what the engine draws around what it says: box rules,
/// the composer, the footer under every turn, a spinner frame a second. This
/// keeps the sentences.
export function readable_from_screen(screen_lines: string[]): string[] {
    const kept: string[] = [];

    for (const source of screen_lines) {
        const line = source.replace(/\s+$/, "");
        const bare = line.trim();

        if (!bare) {
            if (kept.length > 0 && kept[kept.length - 1] !== "") {
                kept.push("");
            }
            continue;
        }

        if (is_chrome(bare)) {
            continue;
        }

        const inside = line
            .replace(/^(\s*)[│┃]\s?/, "$1")
            .replace(/\s?[│┃]$/, "")
            .replace(/\s+$/, "");

        if (!inside.trim()) {
            continue;
        }

        if (kept.length > 0 && kept[kept.length - 1].trim() === inside.trim()) {
            continue;
        }

        kept.push(inside);
    }

    while (kept.length > 0 && kept[0] === "") {
        kept.shift();
    }

    while (kept.length > 0 && kept[kept.length - 1] === "") {
        kept.pop();
    }

    return dedent(kept);
}

/// The engine wraps its own prose to the width of the pane, so a paragraph
/// arrives as a column of short rows. A row that ran to the edge is continued by
/// the next one; joining those back gives sentences that reflow with the panel.
export function unwrap(lines: string[], width: number): string[] {
    const edge = Math.max(24, width - 10);
    const joined: string[] = [];

    for (const line of lines) {
        const previous = joined[joined.length - 1];
        const carries_on =
            previous !== undefined &&
            previous.trim().length > 0 &&
            previous.length >= edge &&
            !/[.:?!]$/.test(previous.trimEnd()) &&
            line.trim().length > 0 &&
            !starts_something(line);

        if (carries_on) {
            joined[joined.length - 1] = `${previous.trimEnd()} ${line.trim()}`;
            continue;
        }

        joined.push(line);
    }

    return joined;
}

function starts_something(line: string): boolean {
    return /^\s*([●○❯⎿⏺>*\-–—]|\d+[.)]|#{1,6}\s|\w+\(|\|)/.test(line);
}

/// The engine indents everything past its left border. Take that margin off and
/// leave the indentation that belongs to the text, so code and diffs keep shape.
function dedent(lines: string[]): string[] {
    let margin = Number.POSITIVE_INFINITY;

    for (const line of lines) {
        if (line.trim()) {
            margin = Math.min(margin, line.length - line.trimStart().length);
        }
    }

    if (!Number.isFinite(margin) || margin === 0) {
        return lines;
    }

    return lines.map((line) => (line.trim() ? line.slice(margin) : line));
}

function is_chrome(line: string): boolean {
    if (/^[─━═╭╮╯╰┌┐└┘│┃|+\-=_.·\s]+$/.test(line)) {
        return true;
    }

    // A rule the engine draws across the pane, whatever it carries in the middle.
    if (/[─━═]{6}/.test(line)) {
        return true;
    }

    // A spinner frame, or the banner drawn from block glyphs.
    if (/^[✻✽✶✢✳⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏]/.test(line) || /^[▐▝▛▜█▀▖▗▘▙▚▞]/.test(line)) {
        return true;
    }

    const lowered = line.toLowerCase();
    return [
        "? for shortcuts",
        "⏵",
        "bypass permissions",
        "shift+tab",
        "esc to interrupt",
        "esc to cancel",
        "ctrl+",
        "context left",
        "auto mode",
        "model:",
        "session:",
        "reset:",
        "● high",
        "❯ try \"",
    ].some((mark) => lowered.startsWith(mark));
}

export function ReadablePane({ screen }: { screen: { current: Screen | null } }) {
    const [lines, set_lines] = useState<string[]>([]);
    const [pinned, set_pinned] = useState(true);
    const holder = useRef<HTMLDivElement>(null);

    use_poll(() => {
        const live = screen.current;
        if (!live) {
            return;
        }

        const width = (live as { cols?: number }).cols ?? 80;
        set_lines(unwrap(readable_from_screen(lines_from_screen(live)), width));
    }, 1000);

    useEffect(() => {
        if (pinned && holder.current) {
            holder.current.scrollTop = holder.current.scrollHeight;
        }
    }, [lines, pinned]);

    return (
        <div
            ref={holder}
            className="min-h-0 flex-1 select-text overflow-y-auto px-3 py-2"
            onScroll={(event) => {
                const box = event.currentTarget;
                set_pinned(box.scrollHeight - box.scrollTop - box.clientHeight < 40);
            }}
        >
            {lines.length === 0 ? (
                <p className="font-mono text-[11px] text-shade">Nothing said yet.</p>
            ) : (
                <div className="flex flex-col gap-1">
                    {lines.map((line, index) =>
                        line === "" ? (
                            <div key={index} className="h-1" />
                        ) : (
                            <p
                                key={index}
                                className="whitespace-pre-wrap break-words text-[12px] leading-relaxed text-driftwood"
                            >
                                {line}
                            </p>
                        ),
                    )}
                </div>
            )}
        </div>
    );
}
