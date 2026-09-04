import { use_poll } from "@/lib/poll";
import { on_a_control } from "@/lib/controls";
import { dated } from "@/lib/dated";
import { use_services } from "@/workspace/registry";

import { exactly, when } from "@/lib/when";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";

import { column_keeps_the_turn, place_of, use_sideways_wheel } from "@/lib/wheel";
import { files_from_paste, is_typing_into } from "@/lib/attachments";

import { marked_copy_of, originals } from "@/lib/marks";

import { AttachmentTile } from "./Attachments";
import { CardEditor } from "./CardEditor";
import { MarkupView } from "./Markup";

import {
    assign_task,
    attachment_object_url,
    delete_task,
    list_agents,
    list_repos,
    list_tasks,
    merge_worktree,
    place_task,
    open_pull_request,
    release_task,
    review_worktree,
    shelved_file,
    type Agent,
    type Column,
    type Entry,
    type Evidence,
    type Repository,
    type Review,
    type Task,
} from "@/lib/core";

const COLUMNS: Column[] = ["backlog", "assigned", "working", "review", "ready", "done"];

/// The panel beside the columns: half the board when the board is wide
/// enough for both, the whole of it when it is not.
export const keeps_the_turn = (target: EventTarget | null) => column_keeps_the_turn(place_of(target));

const ASIDE =
    "flex w-full min-w-0 flex-col border-l border-reef @[820px]:w-[46%] @[820px]:min-w-[380px]";

function patch_line_color(line: string): string {
    if (line.startsWith("+++") || line.startsWith("---") || line.startsWith("diff ")) {
        return "text-shell";
    }
    if (line.startsWith("+")) {
        return "text-palm";
    }
    if (line.startsWith("-")) {
        return "text-coral";
    }
    if (line.startsWith("@@")) {
        return "text-turquoise";
    }
    return "text-driftwood";
}

/// Cards in the order they were placed, oldest id first for the ones written
/// before there was an order to place them in.
function in_order(tasks: Task[]): Task[] {
    return [...tasks].sort((one, other) => {
        const gap = (one.position ?? 0) - (other.position ?? 0);
        return gap !== 0 ? gap : one.id.localeCompare(other.id);
    });
}

export function BoardPanel({ active, repositories }: { active: boolean; repositories: string[] | null }) {
    // The toolbar above the columns turns the wheel into a sideways scroll of
    // the columns; a column with cards below its fold keeps the turn for itself.
    const surface = useRef<HTMLDivElement>(null);
    const columns = use_sideways_wheel<HTMLDivElement>({ surface, keeps: keeps_the_turn });
    const [all_tasks, set_tasks] = useState<Task[]>([]);
    const tasks = repositories
        ? all_tasks.filter((task) => repositories.includes(task.repository_id))
        : all_tasks;
    const [agents, set_agents] = useState<Agent[]>([]);
    const [repos, set_repos] = useState<Repository[]>([]);
    /// The card being written or rewritten, and any files that arrived with
    /// the request to write it — a screenshot pasted onto the board opens the
    /// editor with the screenshot already on the card.
    const [editing, set_editing] = useState<{ task: Task | null; seed: File[] } | null>(null);
    const [review, set_review] = useState<{ task: Task; data: Review } | null>(null);
    /// Where the card being dragged would land: the column, and the card it
    /// would sit above (null meaning the bottom). Drawn, so a drop is aimed
    /// rather than guessed.
    const [aiming, set_aiming] = useState<{ column: Column; before: string | null } | null>(null);

    /// The card in hand, drawn under the pointer.
    ///
    /// The browser's own drag draws nothing here — WebKitGTK gives no drag
    /// image — so the card is carried by hand: a copy follows the pointer and a
    /// gap opens where it would land.
    const [carry, set_carry] = useState<{
        id: string;
        width: number;
        height: number;
        grab_x: number;
        grab_y: number;
        x: number;
        y: number;
    } | null>(null);

    // The aim only changes state when the aim itself changed: pointer moves
    // arrive continuously, and a fresh object each time redrew the whole board
    // dozens of times a second.
    const aim = useCallback((column: Column, before: string | null) => {
        set_aiming((held) =>
            held && held.column === column && held.before === before ? held : { column, before },
        );
    }, []);

    // The drag reads these while it runs. They live in refs so the listeners
    // can be attached once, when the card is picked up: keyed on the carried
    // position instead, they were torn down and rebuilt on every pointer move,
    // which dropped events and made the gap stutter.
    const tasks_now = useRef(tasks);
    tasks_now.current = tasks;
    const carried_now = useRef<string | null>(null);
    const aiming_now = useRef(aiming);
    aiming_now.current = aiming;

    /// Where each column's cards sat when the drag first reached it.
    ///
    /// Measured once per column and not again: reading live positions fed the
    /// drag back into itself — the gap opened, the cards below moved, a
    /// different card was under the pointer, the gap moved back. That loop is
    /// the up-and-down flicker. Boundaries taken before the gap exists do not
    /// move under it.
    const seats = useRef(
        new Map<string, { scrolled: number; cards: Array<{ id: string; middle: number }> }>(),
    );

    /// What is under the pointer: which column, and which card it would sit
    /// above.
    const read_aim = useCallback(
        (x: number, y: number) => {
            const under = document.elementFromPoint(x, y);
            const holder = under?.closest("[data-column]") as HTMLElement | null;
            const column = holder?.getAttribute("data-column");

            if (!holder || !column) {
                return;
            }

            const scroller = holder.querySelector("[data-cards]") as HTMLElement | null;
            const scrolled = scroller?.scrollTop ?? 0;

            // The first move can arrive before the carried card has left the
            // column, and a measurement taken then has a card's worth of space
            // in it that is about to close. Measure again next time rather than
            // keeping that one.
            const still_there = carried_now.current
                ? holder.querySelector(`[data-card="${CSS.escape(carried_now.current)}"]`) !== null
                : false;

            let measured = still_there ? undefined : seats.current.get(column);
            if (!measured) {
                measured = {
                    scrolled,
                    cards: Array.from(holder.querySelectorAll("[data-card]"))
                        .map((card) => {
                            const box = card.getBoundingClientRect();
                            return {
                                id: card.getAttribute("data-card") ?? "",
                                middle: box.top + box.height / 2,
                            };
                        })
                        .filter((card) => card.id && card.id !== carried_now.current),
                };

                if (!still_there) {
                    seats.current.set(column, measured);
                }
            }

            // A column scrolled since it was measured moves its boundaries with
            // it, which is arithmetic rather than another measurement.
            const drift = scrolled - measured.scrolled;
            const above = measured.cards.find((card) => y < card.middle - drift);

            aim(column as Column, above?.id ?? null);
        },
        [aim],
    );

    const take = useCallback((task_id: string, event: React.PointerEvent<HTMLElement>) => {
        const box = event.currentTarget.getBoundingClientRect();
        seats.current.clear();

        set_carry({
            id: task_id,
            width: box.width,
            height: box.height,
            grab_x: event.clientX - box.left,
            grab_y: event.clientY - box.top,
            x: event.clientX,
            y: event.clientY,
        });
    }, []);
    // The id rather than the card: the board polls, and a card held by value
    // would stop changing the moment it was opened.
    const [opened, set_opened] = useState<string | null>(null);
    const [error, set_error] = useState<string | null>(null);
    const [busy, set_busy] = useState(false);

    const refresh = useCallback(async () => {
        const [board, crew, repositories] = await Promise.all([
            list_tasks(),
            list_agents(),
            list_repos(),
        ]);
        set_tasks(board);
        set_agents(crew);
        set_repos(repositories);
    }, []);

    // A drag that ends anywhere — dropped, cancelled, let go over the sidebar —
    // clears the aim. Without this the line stayed drawn and the board stopped
    // refreshing, because it still believed something was being held.
    useEffect(() => {
        const done = () => set_aiming(null);
        window.addEventListener("dragend", done);
        window.addEventListener("drop", done);

        return () => {
            window.removeEventListener("dragend", done);
            window.removeEventListener("drop", done);
        };
    }, []);

    // A refresh in the middle of a drag replaces every card under the pointer.
    use_poll(() => {
        list_tasks().then(set_tasks).catch(() => undefined);
    }, 4000, active && !carry);

    // The board reads the crew and the projects once it is on screen. The
    // editor needs the projects to offer one, and they used to arrive only
    // after the first action on the board.
    useEffect(() => {
        if (active) {
            refresh().catch(() => undefined);
        }
    }, [active, refresh]);

    // A screenshot pasted onto the board is a card waiting to be written: the
    // editor opens with the picture already on it. With a card open for
    // reading, the paste goes onto that card; with the editor open, onto the
    // card being written, wherever the focus happens to be. A paste into a
    // text field is the field's own, and one the editor already took is done.
    useEffect(() => {
        if (!active) {
            return;
        }

        const pasted = (event: ClipboardEvent) => {
            if (event.defaultPrevented || is_typing_into(event.target)) {
                return;
            }
            const files = files_from_paste(event.clipboardData);
            if (files.length === 0) {
                return;
            }
            event.preventDefault();
            set_editing((held) => {
                if (held) {
                    return { ...held, seed: [...held.seed, ...files] };
                }
                const card = opened ? tasks_now.current.find((task) => task.id === opened) ?? null : null;
                return { task: card, seed: files };
            });
        };

        window.addEventListener("paste", pasted);
        return () => window.removeEventListener("paste", pasted);
    }, [active, opened]);

    // A screenshot taken from the tray arrives by name, off the shelf, and
    // goes where a paste would go.
    useEffect(() => {
        const heard = (event: Event) => {
            const command = (event as CustomEvent<string>).detail;
            if (!command.startsWith("shot:")) {
                return;
            }

            shelved_file(command.slice("shot:".length))
                .then((file) => {
                    set_editing((held) => {
                        if (held) {
                            return { ...held, seed: [...held.seed, file] };
                        }
                        const card = opened ? tasks_now.current.find((task) => task.id === opened) ?? null : null;
                        return { task: card, seed: [file] };
                    });
                })
                .catch((cause) => set_error(cause instanceof Error ? cause.message : String(cause)));
        };

        window.addEventListener("agentland:command", heard);
        return () => window.removeEventListener("agentland:command", heard);
    }, [opened]);

    const { open_menu } = use_services();

    const run = useCallback(
        async (action: () => Promise<unknown>) => {
            set_busy(true);
            set_error(null);
            try {
                await action();
                await refresh();
            } catch (cause) {
                set_error(cause instanceof Error ? cause.message : String(cause));
            } finally {
                set_busy(false);
            }
        },
        [refresh],
    );

    // While a card is in hand the whole window follows the pointer, so it keeps
    // up when the pointer leaves the board or the button is let go outside it.
    const carried_id = carry?.id ?? null;
    carried_now.current = carried_id;

    useEffect(() => {
        if (!carried_id) {
            return;
        }

        const moved = (event: PointerEvent) => {
            set_carry((held) => (held ? { ...held, x: event.clientX, y: event.clientY } : held));
            read_aim(event.clientX, event.clientY);
        };

        const released = () => {
            const wanted = aiming_now.current;
            seats.current.clear();
            set_carry(null);
            set_aiming(null);

            if (wanted) {
                void run(() => place_task(carried_id, wanted.column, wanted.before ?? undefined));
            }
        };

        window.addEventListener("pointermove", moved);
        window.addEventListener("pointerup", released);
        window.addEventListener("pointercancel", released);

        return () => {
            window.removeEventListener("pointermove", moved);
            window.removeEventListener("pointerup", released);
            window.removeEventListener("pointercancel", released);
        };
    }, [carried_id, read_aim, run]);


    const open_review = useCallback(async (task: Task) => {
        if (!task.worktree) {
            set_error(`${task.id} has no worktree yet — assign it first`);
            return;
        }
        set_error(null);
        const data = await review_worktree(task.repository_id, task.worktree);
        set_review({ task, data });
    }, []);

    // A panel on the right — a card, its diff, or the editor — shares the
    // width with the columns when there is width to share, and takes all of
    // it when there is not. Measured in the Work preset: at 390px the panel
    // and the columns both got 380px and drew over each other.
    const aside_open = Boolean(editing || review || (opened && tasks.some((task) => task.id === opened)));

    return (
        <div className="@container flex h-full min-h-0 min-w-0 flex-1">
            <div
                ref={surface}
                className={`h-full min-h-0 min-w-0 flex-1 flex-col gap-3 p-2.5 ${
                    aside_open ? "hidden @[820px]:flex" : "flex"
                }`}
            >
                <div className="flex items-center gap-2">
                    <button
                        className="shrink-0 rounded-md border border-turquoise px-2 py-1 font-mono text-[11px] text-turquoise disabled:opacity-40"
                        disabled={busy || repos.length === 0}
                        onClick={() => set_editing({ task: null, seed: [] })}
                        title="write a card: a title, a brief, and the screenshots or files that show it"
                    >
                        + new card
                    </button>
                    <span className="min-w-0 truncate font-mono text-[10px] text-shade">
                        {repos.length === 0
                            ? "add a project first"
                            : "or paste a screenshot anywhere on the board"}
                    </span>
                </div>

                {error ? (
                    <div className="border border-coral bg-lagoon px-2 py-1 font-mono text-[11px] text-coral rounded-lg">
                        {error}
                    </div>
                ) : null}

                <div ref={columns} className="flex min-h-0 flex-1 gap-1.5 overflow-x-auto">
                    {COLUMNS.map((column) => (
                        <div
                            key={column}
                            // The columns share whatever width the panel has and
                            // stop shrinking at a card's worth, at which point
                            // the row scrolls. Fixed at one width they left the rest
                            // of a wide panel empty and pushed "done" off the
                            // edge of a narrow one with room to spare.
                            data-column={column}
                            className={`flex min-h-0 min-w-[220px] flex-1 flex-col rounded-md border bg-lagoon transition-colors ${
                                aiming?.column === column && carry
                                    ? "border-turquoise bg-lagoon-deep"
                                    : "border-reef"
                            }`}
                        >
                            <header className="border-b border-reef px-2 py-1 font-mono text-[11px] uppercase tracking-[0.1em] text-shell">
                                {column} · {tasks.filter((task) => task.column === column).length}
                            </header>

                            <Column
                                // The carried card leaves the list while it is
                                // held: the gap stands in for it, so a column
                                // does not grow by a card and then shrink back.
                                tasks={in_order(
                                    tasks.filter(
                                        (task) => task.column === column && task.id !== carry?.id,
                                    ),
                                )}
                                gap={
                                    carry && aiming?.column === column
                                        ? { before: aiming.before, height: carry.height }
                                        : null
                                }
                                render={(task) => (
                                    <BoardCard
                                        key={task.id}
                                        task={task}
                                        on_take={(event) => take(task.id, event)}
                                        agents={agents}
                                        on_open={() => set_opened(task.id)}
                                        on_assign={(agent_id) => run(() => assign_task(task.id, agent_id))}
                                        on_review={() => void open_review(task)}
                                        on_delete={() => run(() => delete_task(task.id))}
                                        on_menu={(event) => {
                                            const crew_here = agents.filter(
                                                (agent) => agent.repository_id === task.repository_id,
                                            );
                                            open_menu(event, `${task.id} · ${task.title}`, [
                                                { label: "Open", run: () => set_opened(task.id) },
                                                ...(task.worktree
                                                    ? [{ label: "Review the work", run: () => void open_review(task) }]
                                                    : []),
                                                {
                                                    label: "Move to",
                                                    items: COLUMNS.filter((column) => column !== task.column).map(
                                                        (column) => ({
                                                            label: column,
                                                            run: () => run(() => place_task(task.id, column)),
                                                        }),
                                                    ),
                                                },
                                                {
                                                    label: "Hand to",
                                                    disabled: crew_here.length === 0,
                                                    hint: crew_here.length === 0 ? "nobody hired here" : undefined,
                                                    items: crew_here.map((agent) => ({
                                                        label: agent.name,
                                                        hint: agent.role,
                                                        disabled: agent.id === task.assignee,
                                                        run: () => run(() => assign_task(task.id, agent.id)),
                                                    })),
                                                },
                                                ...(task.assignee
                                                    ? [
                                                          {
                                                              label: `Take back from ${task.assignee}`,
                                                              hint: "it returns to the backlog",
                                                              run: () => run(() => release_task(task.id)),
                                                          },
                                                      ]
                                                    : []),
                                                {
                                                    label: "Delete",
                                                    danger: true,
                                                    run: () => run(() => delete_task(task.id)),
                                                },
                                            ]);
                                        }}
                                    />
                                )}
                            />
                        </div>
                    ))}
                </div>
            </div>

            {carry
                ? (() => {
                      const held = tasks.find((task) => task.id === carry.id);
                      if (!held) {
                          return null;
                      }

                      // The card itself, under the pointer, tilted a little so
                      // it reads as picked up rather than as part of the board.
                      // It takes no pointer events, or it would be what the
                      // pointer is over and nothing else could be aimed at.
                      return (
                          <div
                              className="pointer-events-none fixed left-0 top-0 z-50 rounded-lg border border-turquoise bg-lagoon-deep p-2 opacity-95 shadow-[0_10px_24px_rgba(0,0,0,0.45)] will-change-transform"
                              style={{
                                  left: 0,
                                  top: 0,
                                  width: carry.width,
                                  transform: `translate3d(${carry.x - carry.grab_x}px, ${carry.y - carry.grab_y}px, 0) rotate(2deg)`,
                              }}
                          >
                              <div className="flex items-baseline justify-between gap-2">
                                  <span className="text-[11px] text-linen">{held.title}</span>
                                  <span className="font-mono text-[10px] text-shade">{held.id}</span>
                              </div>
                              {held.branch ? (
                                  <p className="mt-1 font-mono text-[10px] text-driftwood">
                                      {held.branch}
                                  </p>
                              ) : null}
                          </div>
                      );
                  })()
                : null}

            {editing ? (
                <CardEditor
                    key={editing.task?.id ?? "new"}
                    task={editing.task}
                    repos={repos}
                    default_repository={
                        (repositories ? repos.find((repo) => repositories.includes(repo.id)) : repos[0])?.id ??
                        repos[0]?.id ??
                        ""
                    }
                    seed={editing.seed}
                    on_close={() => set_editing(null)}
                    on_saved={(saved) => {
                        set_editing(null);
                        set_opened(saved.id);
                        void refresh();
                    }}
                />
            ) : null}

            {!editing && !review && opened && tasks.some((task) => task.id === opened) ? (
                <CardDetail
                    task={tasks.find((task) => task.id === opened)!}
                    on_close={() => set_opened(null)}
                    on_edit={() => {
                        const held = tasks.find((task) => task.id === opened);
                        if (held) {
                            set_editing({ task: held, seed: [] });
                        }
                    }}
                    on_changed={() => void refresh()}
                    on_review={() => {
                        const held = tasks.find((task) => task.id === opened);
                        if (held) {
                            void open_review(held);
                        }
                    }}
                    on_merge={() => {
                        const held = tasks.find((task) => task.id === opened);
                        if (held?.worktree) {
                            void run(() =>
                                merge_worktree(held.repository_id, held.worktree!, held.id),
                            );
                        }
                    }}
                />
            ) : null}

            {!editing && review ? (
                <aside className={`${ASIDE} @[820px]:min-w-[440px]`}>
                    <header className="flex items-center justify-between gap-2 border-b border-reef px-2 py-1">
                        <div className="font-mono text-[11px] text-shell">
                            {review.data.branch} vs {review.data.base} · {review.data.files} files ·{" "}
                            <span className="text-palm">+{review.data.insertions}</span>{" "}
                            <span className="text-coral">-{review.data.deletions}</span>
                            {review.data.uncommitted ? " · uncommitted work" : ""}
                        </div>
                        <div className="flex gap-2">
                            <button
                                className="border border-turquoise px-2 py-1 font-mono text-[11px] text-turquoise disabled:opacity-40 rounded-lg"
                                disabled={busy}
                                onClick={() =>
                                    run(async () => {
                                        const result = await open_pull_request(
                                            review.task.repository_id,
                                            review.task.worktree as string,
                                            review.task.title,
                                            review.task.body,
                                            review.task.id,
                                        );
                                        set_error(`${result.detail}: ${result.url}`);
                                    })
                                }
                            >
                                open pull request
                            </button>
                            <button
                                className="border border-foam px-2 py-1 font-mono text-[11px] rounded-lg"
                                onClick={() => set_review(null)}
                            >
                                close
                            </button>
                        </div>
                    </header>

                    {review.data.commits.length > 0 ? (
                        <div className="border-b border-reef px-2 py-1 font-mono text-[11px] text-driftwood">
                            {review.data.commits.map((commit) => (
                                <div key={commit.sha}>
                                    <span className="text-turquoise">{commit.sha}</span> {commit.subject}
                                </div>
                            ))}
                        </div>
                    ) : null}

                    <pre className="min-h-0 flex-1 overflow-auto p-2 font-mono text-[11px] leading-relaxed">
                        {review.data.patch.split("\n").map((line, index) => (
                            <div key={index} className={patch_line_color(line)}>
                                {line || " "}
                            </div>
                        ))}
                    </pre>
                </aside>
            ) : null}
        </div>
    );
}

/// A column that draws only the cards in view.
///
/// Measured on a board of 325: every card in the DOM cost 12 fps with the panel
/// on screen. Only what fits is rendered, plus a few rows either side so a
/// scroll never shows a gap.
function Column({
    tasks,
    gap,
    render,
}: {
    tasks: Task[];
    /// Where the carried card would land, and how tall it is: the cards below
    /// step down by that much and the space between them is drawn, so the drop
    /// is aimed at a place rather than at a line.
    gap: { before: string | null; height: number } | null;
    render: (task: Task) => ReactNode;
}) {
    const holder = useRef<HTMLDivElement>(null);

    const rows = useVirtualizer({
        count: tasks.length,
        getScrollElement: () => holder.current,
        estimateSize: () => 96,
        overscan: 6,
    });

    const seat = gap
        ? gap.before
            ? Math.max(0, tasks.findIndex((task) => task.id === gap.before))
            : tasks.length
        : -1;
    const room = gap ? gap.height + 8 : 0;
    const shown = rows.getVirtualItems();
    const gap_top =
        seat < 0
            ? 0
            : seat >= tasks.length
              ? rows.getTotalSize()
              : (shown.find((row) => row.index === seat)?.start ?? seat * 96);

    return (
        <div ref={holder} data-cards className="min-h-0 flex-1 overflow-y-auto p-2">
            <div className="relative w-full" style={{ height: rows.getTotalSize() + room }}>
                {gap ? (
                    <div
                        className="pointer-events-none absolute inset-x-0 rounded-lg border border-dashed border-turquoise/70 bg-turquoise/5"
                        style={{ transform: `translateY(${gap_top}px)`, height: gap.height }}
                    />
                ) : null}

                {shown.map((row) => (
                    <div
                        key={tasks[row.index].id}
                        ref={rows.measureElement}
                        data-index={row.index}
                        className="absolute inset-x-0 pb-2"
                        style={{
                            transform: `translateY(${row.start + (seat >= 0 && row.index >= seat ? room : 0)}px)`,
                        }}
                    >
                        {render(tasks[row.index])}
                    </div>
                ))}
            </div>
        </div>
    );
}

const KIND_TINT: Record<string, string> = {
    finished: "text-palm",
    commit: "text-turquoise",
    diff: "text-shell",
    pull_request: "text-sun",
    note: "text-shade",
};

/// The evidence inside an entry, whichever shape it arrived in.
///
/// A core that has not been restarted since the board learned to record who did
/// what still serves bare evidence with no `what` around it. The window and the
/// core are separate processes and one can be older than the other, so the
/// reader takes both rather than showing an empty history for a version skew.
function what_of(entry: Entry): Evidence {
    return entry.what ?? (entry as unknown as Evidence);
}

/// One line of a card's history, in the words of whoever wrote it.
function said(entry: Entry): string {
    const what = what_of(entry);
    switch (what.kind) {
        case "commit":
            return `${String(what.sha).slice(0, 7)} ${what.subject}`;
        case "diff":
            return `${what.files} files · +${what.insertions} −${what.deletions}`;
        case "pull_request":
            return String(what.url);
        case "finished": {
            const touched = Number(what.files ?? 0);
            const size = touched
                ? ` · ${touched} file${touched === 1 ? "" : "s"} +${what.insertions} −${what.deletions}`
                : "";
            return `${what.summary}${size}`;
        }
        default:
            return String(what.text ?? what.kind);
    }
}

/// Everything the card knows about itself: what was asked, who took it, where
/// they worked, and what each of them left behind.
///
/// A card used to say "3 evidence" and nothing more, which is the count of an
/// answer rather than the answer.
function CardDetail({
    task,
    on_close,
    on_edit,
    on_changed,
    on_review,
    on_merge,
}: {
    task: Task;
    on_close: () => void;
    on_edit: () => void;
    on_changed: () => void;
    on_review: () => void;
    on_merge: () => void;
}) {
    const now = Math.floor(Date.now() / 1000);
    const finish = task.evidence.filter((entry) => what_of(entry).kind === "finished").at(-1);
    const attachments = originals(task.attachments);
    const [shown, set_shown] = useState<string | null>(null);
    const loader_for = useCallback(
        (name: string) => () => attachment_object_url(task.id, name),
        [task.id],
    );

    return (
        <aside className={ASIDE}>
            <header className="flex items-start justify-between gap-2 border-b border-reef px-2 py-1.5">
                <div className="min-w-0">
                    <div className="text-[12px] text-linen">{task.title}</div>
                    <div className="font-mono text-[10px] text-shade" title={exactly(task.at ?? 0)}>
                        {task.id} · {task.column} · written {when(task.at ?? 0, now)}
                    </div>
                </div>
                <div className="flex shrink-0 items-center gap-1">
                    <button
                        className="rounded-lg border border-reef px-2 py-0.5 font-mono text-[11px] text-driftwood hover:text-linen"
                        onClick={on_edit}
                        title="rewrite the card, or put a screenshot on it"
                    >
                        edit
                    </button>
                    <button
                        className="rounded px-1.5 font-mono text-[11px] text-shell hover:text-linen"
                        onClick={on_close}
                    >
                        ✕
                    </button>
                </div>
            </header>

            <div className="flex min-h-0 flex-1 flex-col gap-2.5 overflow-y-auto p-2.5">
                {task.body.trim() ? (
                    <p className="whitespace-pre-wrap text-[11px] text-shell">{task.body}</p>
                ) : null}

                {attachments.length > 0 ? (
                    <section>
                        <h3 className="mb-1 font-mono text-[9px] uppercase tracking-[0.14em] text-shade">
                            Attached · {attachments.length}
                        </h3>
                        <div className="flex flex-wrap gap-1.5">
                            {attachments.map((held) => (
                                <AttachmentTile
                                    key={held.name}
                                    id={`${task.id}/${held.name}`}
                                    name={held.name}
                                    kind={held.kind}
                                    bytes={held.bytes}
                                    load={loader_for(held.name)}
                                    marked={held.marks?.marks.length}
                                    on_open={() => (held.kind.startsWith("image/") ? set_shown(held.name) : undefined)}
                                />
                            ))}
                        </div>
                        <p className="mt-1 font-mono text-[9px] text-shade">
                            whoever takes this card is handed these by path, and reads them · click a picture to draw on it
                        </p>
                    </section>
                ) : null}

                <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 font-mono text-[10px]">
                    <dt className="text-shade">who</dt>
                    <dd className="text-linen">{task.assignee ?? "nobody yet"}</dd>
                    <dt className="text-shade">where</dt>
                    <dd className="text-linen">{task.worktree ?? "not bound to a worktree"}</dd>
                    <dt className="text-shade">branch</dt>
                    <dd className="text-turquoise">{task.branch ?? "none yet"}</dd>
                    <dt className="text-shade">project</dt>
                    <dd className="text-linen">{task.repository_id}</dd>
                </dl>

                {finish ? (
                    <section className="rounded-md border border-palm/60 bg-lagoon-deep px-2 py-1.5">
                        <h3 className="font-mono text-[9px] uppercase tracking-[0.14em] text-palm">
                            How it ended
                        </h3>
                        <p className="mt-0.5 text-[11px] text-linen">{said(finish)}</p>
                        <p className="font-mono text-[10px] text-shade" title={exactly(finish.at)}>
                            {finish.by ?? "someone"} · {finish.at ? when(finish.at, now) : "no date"}
                        </p>
                    </section>
                ) : null}

                <section>
                    <h3 className="mb-1 font-mono text-[9px] uppercase tracking-[0.14em] text-shade">
                        What happened · {task.evidence.length}
                    </h3>

                    {task.evidence.length === 0 ? (
                        <p className="font-mono text-[10px] text-shade">
                            Nothing has been recorded on this card yet.
                        </p>
                    ) : null}

                    <ol className="flex flex-col gap-1">
                        {task.evidence.map((entry, index) => (
                            <li
                                key={`${entry.at}-${index}`}
                                className="rounded-md border border-reef bg-lagoon-deep px-2 py-1"
                            >
                                <div className={`text-[11px] ${KIND_TINT[what_of(entry).kind] ?? "text-shell"}`}>
                                    {said(entry)}
                                </div>
                                <div
                                    className="font-mono text-[10px] text-shade"
                                    title={entry.at ? exactly(entry.at) : "before this was recorded"}
                                >
                                    {what_of(entry).kind} · {entry.by ?? "someone"} ·{" "}
                                    {entry.at ? when(entry.at, now) : "no date"}
                                </div>
                            </li>
                        ))}
                    </ol>
                </section>

                <div className="flex flex-wrap gap-2">
                    {task.worktree ? (
                        <button
                            className="rounded-lg border border-turquoise px-2 py-0.5 font-mono text-[11px] text-turquoise"
                            onClick={on_review}
                        >
                            read the diff
                        </button>
                    ) : null}

                    {task.column === "ready" && task.worktree ? (
                        <button
                            className="rounded-lg border border-palm px-2 py-0.5 font-mono text-[11px] text-palm"
                            onClick={on_merge}
                            title="squash and merge the pull request, and finish this card"
                        >
                            merge it
                        </button>
                    ) : null}
                </div>
            </div>

            {shown && attachments.some((held) => held.name === shown) ? (
                <MarkupView
                    key={shown}
                    task_id={task.id}
                    attachment={attachments.find((held) => held.name === shown)!}
                    copy={marked_copy_of(task.attachments, shown)}
                    load={loader_for(shown)}
                    on_close={() => set_shown(null)}
                    on_saved={() => {
                        set_shown(null);
                        on_changed();
                    }}
                />
            ) : null}
        </aside>
    );
}

function BoardCard({
    task,
    agents,
    on_take,
    on_open,
    on_assign,
    on_review,
    on_delete,
    on_menu,
}: {
    task: Task;
    agents: Agent[];
    on_take?: (event: React.PointerEvent<HTMLElement>) => void;
    on_open: () => void;
    on_assign: (agent_id: string) => void;
    on_review: () => void;
    on_delete: () => void;
    on_menu?: (event: React.MouseEvent) => void;
}) {
    return (
        <article
                            key={task.id}
                            data-card={task.id}
                            // Taken with the pointer rather than by the browser's
                            // own drag, which draws no image at all here: the
                            // card would be dragged and nothing would follow.
                            // A few pixels of movement separate carrying it from
                            // clicking it open.
                            onPointerDown={(event) => {
                                if (event.button !== 0 || on_a_control(event.target)) {
                                    return;
                                }

                                const from = { x: event.clientX, y: event.clientY };
                                const target = event.currentTarget;

                                const watch = (moved: PointerEvent) => {
                                    if (
                                        Math.abs(moved.clientX - from.x) +
                                            Math.abs(moved.clientY - from.y) >
                                        4
                                    ) {
                                        stop();
                                        on_take?.({
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
                            onClick={(event) => {
                                if (!on_a_control(event.target)) {
                                    on_open();
                                }
                            }}
                            onContextMenu={(event) => on_menu?.(event)}
                            className="cursor-grab select-none rounded-lg border border-reef bg-lagoon p-2"
                        >
                            <div className="flex items-baseline justify-between gap-2">
                                <span className="text-[11px] text-linen">{task.title}</span>
                                <span
                                    className="font-mono text-[10px] text-shade"
                                    title={exactly(dated(task.at, task.evidence))}
                                >
                                    {task.id} · {when(dated(task.at, task.evidence), Math.floor(Date.now() / 1000))}
                                </span>
                            </div>
        
                            {task.branch ? (
                                <div className="mt-1 font-mono text-[10px] text-turquoise">
                                    {task.branch}
                                </div>
                            ) : null}
        
                            {originals(task.attachments).length > 0 ? (
                                <div className="mt-1 font-mono text-[10px] text-shell">
                                    {originals(task.attachments).length} attached
                                    {originals(task.attachments).some((held) => held.marks?.marks.length)
                                        ? " · marked up"
                                        : ""}
                                </div>
                            ) : null}

                            {task.evidence.length > 0 ? (
                                <div className="mt-1 font-mono text-[10px] text-palm">
                                    {task.evidence.some((entry) => what_of(entry).kind === "finished")
                                        ? "finished · "
                                        : ""}
                                    {task.evidence.length} on its history
                                </div>
                            ) : null}
        
                            <div className="mt-2 flex flex-wrap gap-1">
                                {task.assignee ? (
                                    <span className="border border-reef px-1 font-mono text-[10px] text-driftwood rounded-lg">
                                        {task.assignee}
                                    </span>
                                ) : (
                                    <select
                                        className="border border-reef bg-lagoon-deep px-1 font-mono text-[10px] rounded-lg"
                                        value=""
                                        onChange={(event) =>
                                            on_assign(event.target.value)
                                        }
                                    >
                                        <option value="">assign…</option>
                                        {agents
                                            .filter(
                                                (agent) =>
                                                    agent.repository_id === task.repository_id,
                                            )
                                            .map((agent) => (
                                                <option key={agent.id} value={agent.id}>
                                                    {agent.name}
                                                </option>
                                            ))}
                                    </select>
                                )}
        
                                {task.worktree ? (
                                    <button
                                        className="border border-reef px-1 font-mono text-[10px] text-driftwood rounded-lg"
                                        onClick={() => on_review()}
                                    >
                                        review
                                    </button>
                                ) : null}
        
                                <button
                                    className="border border-reef px-1 font-mono text-[10px] text-shell rounded-lg"
                                    onClick={() => on_delete()}
                                >
                                    delete
                                </button>
                            </div>
                        </article>
    );
}
