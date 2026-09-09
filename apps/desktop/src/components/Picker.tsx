import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";

import { first_pickable, narrow, shown, step, WORTH_SEARCHING, type Choice } from "@/lib/picking";

/// A dropdown you can type at.
///
/// A native one answers a keystroke by jumping to the next thing that starts
/// with that letter, which is no help at all once the list is a crew, every
/// worktree in a project, or every model an engine offers. This narrows as
/// somebody types, and below a handful of rows it does not offer the box at
/// all — six rows are read faster than a word is typed.
///
/// It hangs from the window rather than from the box it belongs to: nearly
/// every list in this app sits in a panel that scrolls, and a list drawn inside
/// one is a list cut off at the panel's edge.
export function Picker({
    value,
    choices,
    on_pick,
    placeholder = "pick one",
    className = "",
    title,
    disabled = false,
    id,
}: {
    value: string;
    choices: Choice[];
    on_pick: (value: string) => void;
    placeholder?: string;
    className?: string;
    title?: string;
    disabled?: boolean;
    id?: string;
}) {
    const [open, set_open] = useState(false);
    const [typed, set_typed] = useState("");
    const [active, set_active] = useState(0);
    const [where, set_where] = useState<{ left: number; top: number; width: number; below: boolean } | null>(null);

    const trigger = useRef<HTMLButtonElement>(null);
    const popup = useRef<HTMLDivElement>(null);
    const searching = choices.length >= WORTH_SEARCHING;
    const narrowed = useMemo(() => narrow(choices, typed), [choices, typed]);

    const shut = useCallback(() => {
        set_open(false);
        set_typed("");
        trigger.current?.focus();
    }, []);

    const pick = useCallback(
        (choice: Choice) => {
            if (choice.disabled) {
                return;
            }

            on_pick(choice.value);
            shut();
        },
        [on_pick, shut],
    );

    // Where to draw it, measured from the box it belongs to. Above instead of
    // below when there is no room under it — a picker at the bottom of a panel
    // would otherwise open off the screen.
    const place = useCallback(() => {
        const box = trigger.current?.getBoundingClientRect();
        if (!box) {
            return;
        }

        const under = window.innerHeight - box.bottom;
        set_where({
            left: box.left,
            top: under < 240 && box.top > under ? box.top : box.bottom + 2,
            width: box.width,
            below: !(under < 240 && box.top > under),
        });
    }, []);

    useLayoutEffect(() => {
        if (!open) {
            return;
        }

        place();
        set_active(first_pickable(narrowed));
    }, [open, place]);

    // A list that stayed where it was drawn while the panel under it moved is
    // a list pointing at the wrong row, so it closes rather than follows.
    useEffect(() => {
        if (!open) {
            return;
        }

        const away = (event: PointerEvent) => {
            const at = event.target as Node;
            if (!popup.current?.contains(at) && !trigger.current?.contains(at)) {
                set_open(false);
                set_typed("");
            }
        };

        const moved = () => {
            set_open(false);
            set_typed("");
        };

        document.addEventListener("pointerdown", away, true);
        window.addEventListener("scroll", moved, true);
        window.addEventListener("resize", moved);
        return () => {
            document.removeEventListener("pointerdown", away, true);
            window.removeEventListener("scroll", moved, true);
            window.removeEventListener("resize", moved);
        };
    }, [open]);

    useEffect(() => {
        set_active(first_pickable(narrowed));
    }, [typed, narrowed.length]);

    const on_keys = useCallback(
        (event: React.KeyboardEvent) => {
            if (event.key === "Escape") {
                event.preventDefault();
                shut();
                return;
            }

            if (event.key === "ArrowDown" || event.key === "ArrowUp") {
                event.preventDefault();
                set_active((at) => step(narrowed, at, event.key === "ArrowDown" ? 1 : -1));
                return;
            }

            if (event.key === "Enter") {
                event.preventDefault();
                const held = narrowed[active];
                if (held) {
                    pick(held);
                }
                return;
            }

            if (event.key === "Tab") {
                shut();
            }
        },
        [active, narrowed, pick, shut],
    );

    return (
        <>
            <button
                ref={trigger}
                id={id}
                type="button"
                role="combobox"
                aria-expanded={open}
                aria-haspopup="listbox"
                disabled={disabled}
                title={title}
                className={`flex items-center gap-1 text-left disabled:opacity-60 ${className}`}
                onClick={() => (open ? shut() : set_open(true))}
                onKeyDown={(event) => {
                    if (!open && (event.key === "ArrowDown" || event.key === "Enter")) {
                        event.preventDefault();
                        set_open(true);
                    }
                }}
            >
                <span className="min-w-0 flex-1 truncate">{shown(choices, value, placeholder)}</span>
                <svg viewBox="0 0 12 8" className="h-2 w-2.5 shrink-0" aria-hidden="true">
                    <path
                        d="M1 1.5 6 6.5 11 1.5"
                        fill="none"
                        stroke="currentColor"
                        strokeWidth="1.6"
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        className="text-shade"
                    />
                </svg>
            </button>

            {open && where
                ? createPortal(
                      <div
                          ref={popup}
                          className="fixed z-50 flex flex-col overflow-hidden rounded-lg border border-foam bg-lagoon-deep shadow-lg"
                          style={{
                              left: where.left,
                              top: where.below ? where.top : undefined,
                              bottom: where.below ? undefined : window.innerHeight - where.top + 2,
                              minWidth: where.width,
                              maxWidth: Math.max(where.width, 360),
                          }}
                          onKeyDown={on_keys}
                      >
                          {searching ? (
                              <input
                                  autoFocus
                                  className="border-b border-reef bg-lagoon px-2 py-1 font-mono text-[11px]"
                                  placeholder="type to narrow"
                                  value={typed}
                                  onChange={(event) => set_typed(event.target.value)}
                              />
                          ) : null}

                          <div
                              role="listbox"
                              tabIndex={searching ? -1 : 0}
                              ref={(node) => {
                                  if (node && !searching) {
                                      node.focus();
                                  }
                              }}
                              className="max-h-64 min-w-0 overflow-y-auto py-0.5 outline-none"
                          >
                              {narrowed.length === 0 ? (
                                  <p className="px-2 py-1 font-mono text-[10px] text-shade">
                                      nothing here answers to that
                                  </p>
                              ) : null}

                              {narrowed.map((choice, at) => (
                                  <div
                                      key={choice.value}
                                      role="option"
                                      aria-selected={choice.value === value}
                                      aria-disabled={choice.disabled}
                                      className={`flex cursor-pointer items-baseline gap-2 px-2 py-1 font-mono text-[11px] ${
                                          choice.disabled
                                              ? "cursor-default text-shade"
                                              : at === active
                                                ? "bg-shallow text-linen"
                                                : "text-shell"
                                      }`}
                                      onPointerEnter={() => !choice.disabled && set_active(at)}
                                      onClick={() => pick(choice)}
                                  >
                                      <span className="min-w-0 flex-1 truncate">
                                          {choice.value === value ? "✓ " : ""}
                                          {choice.label}
                                      </span>
                                      {choice.hint ? (
                                          <span className="shrink-0 text-[10px] text-shade">{choice.hint}</span>
                                      ) : null}
                                  </div>
                              ))}
                          </div>
                      </div>,
                      document.body,
                  )
                : null}
        </>
    );
}
