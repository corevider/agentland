import { useCallback, useEffect, useMemo, useState } from "react";

import {
    begin_project,
    list_engines,
    list_starters,
    type Begun,
    type Engine,
    type Starter,
} from "@/lib/core";
import { Spinner, Waiting } from "@/components/Spinner";
import { name_trouble } from "@/lib/naming";
import { as_url, clone_target, is_clonable, pick_folder } from "@/lib/pick";
import { use_services } from "@/workspace/registry";

type Where = "new" | "folder" | "clone";

const WHERE_LABEL: Record<Where, string> = {
    new: "something new",
    folder: "a folder on this machine",
    clone: "clone a repository",
};

/// Everything a project needs, in the order it needs it.
///
/// A workspace, a project, a worktree, an agent and a brief were each their own
/// panel, and nothing said which came first — a person who guessed wrong met an
/// error about a worktree instead of a crew at work. This asks the two questions
/// that are genuinely theirs, where the work is and what it is, and lets the
/// core decide the rest.
///
/// When the work does not exist yet there is a third question, and it is the one
/// that is expensive to get wrong: what to build it out of. The versions shown
/// beside each answer are asked of npm and cargo when the panel opens, and the
/// commands are shown before anything runs, because running them downloads and
/// executes other people's code.
export function StartPanel({ active }: { active: boolean }) {
    const { open_session } = use_services();
    const [where, set_where] = useState<Where>("new");
    const [path, set_path] = useState("");
    const [url, set_url] = useState("");
    const [into, set_into] = useState("");
    const [name, set_name] = useState("");
    const [stack, set_stack] = useState<string | null>(null);
    const [extras, set_extras] = useState<string[]>([]);
    const [starters, set_starters] = useState<Starter[] | null>(null);
    const [goal, set_goal] = useState("");
    const [engines, set_engines] = useState<Engine[]>([]);
    const [engine_id, set_engine] = useState("");
    const [workspace, set_workspace] = useState("");
    const [worktree, set_worktree] = useState("");
    const [commander, set_commander] = useState("");
    const [showing_more, set_showing_more] = useState(false);
    const [needs_git, set_needs_git] = useState(false);
    const [error, set_error] = useState<string | null>(null);
    const [busy, set_busy] = useState(false);
    const [begun, set_begun] = useState<Begun | null>(null);

    useEffect(() => {
        if (!active) {
            return;
        }

        list_engines()
            .then((known) => set_engines(known.filter((engine) => engine.installed)))
            .catch(() => undefined);
    }, [active]);

    // The versions cost a process each and go stale in a week, so they are asked
    // when the panel opens and not on every keystroke of the name.
    useEffect(() => {
        if (!active || where !== "new" || starters) {
            return;
        }

        list_starters()
            .then(set_starters)
            .catch((cause) => set_error(cause instanceof Error ? cause.message : String(cause)));
    }, [active, where, starters]);

    const trouble = useMemo(() => (name ? name_trouble(name) : null), [name]);
    const chosen = useMemo(
        () => starters?.find((entry) => entry.id === stack) ?? null,
        [starters, stack],
    );

    const ready = useMemo(() => {
        if (goal.trim().length === 0) {
            return false;
        }
        if (where === "folder") {
            return path.trim().length > 0;
        }
        if (where === "clone") {
            return is_clonable(url) && into.trim().length > 0;
        }
        return (
            path.trim().length > 0 &&
            name.trim().length > 0 &&
            trouble === null &&
            chosen !== null &&
            chosen.installed
        );
    }, [goal, where, path, url, into, name, trouble, chosen]);

    // A box that looks like a checkbox and cannot be reached with a keyboard is
    // a box half the people who need it cannot tick.
    const toggle_extra = useCallback((id: string) => {
        set_extras((held) => (held.includes(id) ? held.filter((entry) => entry !== id) : [...held, id]));
    }, []);

    const start = useCallback(
        async (start_git: boolean) => {
            set_busy(true);
            set_error(null);
            set_needs_git(false);

            const somewhere =
                where === "clone"
                    ? { url: as_url(url), into: into.trim() }
                    : where === "folder"
                      ? { path: path.trim(), start_git }
                      : {
                            path: path.trim(),
                            stack: stack ?? undefined,
                            name: name.trim(),
                            ...(extras.length > 0 ? { extras } : {}),
                        };

            try {
                const done = await begin_project({
                    goal: goal.trim(),
                    ...somewhere,
                    ...(workspace.trim() ? { workspace: workspace.trim() } : {}),
                    ...(worktree.trim() ? { worktree: worktree.trim() } : {}),
                    ...(engine_id ? { engine_id } : {}),
                    ...(commander.trim() ? { commander: commander.trim() } : {}),
                });

                set_begun(done);
                if (done.commander.session_id) {
                    open_session(done.commander.session_id);
                }
            } catch (cause) {
                const said = cause instanceof Error ? cause.message : String(cause);
                // A folder that is not a repository yet is not a mistake — it is
                // the other half of the question, and `git init` writes into
                // somebody's folder, so it waits for a yes.
                if (said.includes("not a git repository")) {
                    set_needs_git(true);
                } else {
                    set_error(said);
                }
            } finally {
                set_busy(false);
            }
        },
        [goal, where, path, url, into, stack, extras, name, workspace, worktree, engine_id, commander, open_session],
    );

    if (begun) {
        return (
            <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-2.5 overflow-y-auto p-2.5">
                <section className="flex flex-col gap-2 rounded-lg border border-turquoise bg-lagoon-deep p-2">
                    <h3 className="font-mono text-[9px] uppercase tracking-[0.14em] text-shade">
                        {begun.commander.name} is on it
                    </h3>
                    <ul className="flex flex-col gap-1">
                        {begun.did.map((line) => (
                            <li key={line} className="font-mono text-[11px] text-shell">
                                · {line}
                            </li>
                        ))}
                    </ul>
                    {begun.vetting && !begun.vetting.ran ? (
                        <p className="font-mono text-[10px] text-sun">
                            Nobody looked for known vulnerabilities — install {begun.vetting.tool} and
                            an audit runs with every project you start here.
                        </p>
                    ) : null}
                    <p className="font-mono text-[10px] text-shade">
                        {begun.repository.name} · {begun.worktree.branch} · port {begun.worktree.port} ·
                        workspace {begun.workspace.name}
                    </p>
                    <div className="flex flex-wrap gap-2">
                        <button
                            className="rounded-lg border border-turquoise px-3 py-1 font-mono text-[11px] text-turquoise disabled:opacity-40"
                            disabled={!begun.commander.session_id}
                            onClick={() =>
                                begun.commander.session_id && open_session(begun.commander.session_id)
                            }
                        >
                            open {begun.commander.name}'s pane
                        </button>
                        <button
                            className="rounded-lg border border-reef px-3 py-1 font-mono text-[11px] text-shell hover:border-foam"
                            onClick={() => {
                                set_begun(null);
                                set_goal("");
                                set_name("");
                            }}
                        >
                            start another
                        </button>
                    </div>
                </section>
            </div>
        );
    }

    return (
        <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-2.5 overflow-y-auto p-2.5">
            <section className="flex flex-col gap-2 rounded-lg border border-reef bg-lagoon-deep p-2">
                <h3 className="font-mono text-[9px] uppercase tracking-[0.14em] text-shade">
                    Where the work is
                </h3>

                <div className="flex flex-wrap gap-2">
                    {(["new", "folder", "clone"] as Where[]).map((choice) => (
                        <button
                            key={choice}
                            className={`rounded-lg border px-2 py-[3px] font-mono text-[11px] ${
                                where === choice
                                    ? "border-turquoise text-turquoise"
                                    : "border-reef text-shell hover:border-foam"
                            }`}
                            onClick={() => {
                                set_where(choice);
                                set_needs_git(false);
                            }}
                        >
                            {WHERE_LABEL[choice]}
                        </button>
                    ))}
                </div>

                {where === "new" ? (
                    <div className="flex flex-col gap-2">
                        <div className="flex flex-wrap items-center gap-2">
                            <input
                                className="min-w-[10rem] flex-1 rounded-lg border border-reef bg-lagoon px-2 py-1 font-mono text-[11px]"
                                placeholder="what to call it"
                                value={name}
                                onChange={(event) => set_name(event.target.value)}
                            />
                            <input
                                className="min-w-[14rem] flex-1 rounded-lg border border-reef bg-lagoon px-2 py-1 font-mono text-[11px]"
                                placeholder="the folder to put it under"
                                value={path}
                                onChange={(event) => set_path(event.target.value)}
                            />
                            <button
                                className="rounded-lg border border-reef px-3 py-1 font-mono text-[11px] text-shell hover:border-foam"
                                onClick={async () => {
                                    const picked = await pick_folder("Put the project under…", path || undefined);
                                    if (picked) {
                                        set_path(picked);
                                    }
                                }}
                            >
                                browse…
                            </button>
                        </div>
                        {trouble ? <p className="font-mono text-[10px] text-coral">{trouble}</p> : null}
                        {name && !trouble && path.trim() ? (
                            <p className="font-mono text-[10px] text-shade">
                                lands in {path.trim().replace(/\/$/, "")}/{name}
                            </p>
                        ) : null}
                    </div>
                ) : where === "folder" ? (
                    <div className="flex flex-wrap items-center gap-2">
                        <input
                            className="min-w-[18rem] flex-1 rounded-lg border border-reef bg-lagoon px-2 py-1 font-mono text-[11px]"
                            placeholder="a folder on this machine"
                            value={path}
                            onChange={(event) => {
                                set_path(event.target.value);
                                set_needs_git(false);
                            }}
                        />
                        <button
                            className="rounded-lg border border-reef px-3 py-1 font-mono text-[11px] text-shell hover:border-foam"
                            onClick={async () => {
                                const picked = await pick_folder("Start a project here", path || undefined);
                                if (picked) {
                                    set_path(picked);
                                    set_needs_git(false);
                                }
                            }}
                        >
                            browse…
                        </button>
                    </div>
                ) : (
                    <div className="flex flex-col gap-2">
                        <div className="flex flex-wrap items-center gap-2">
                            <input
                                className="min-w-[18rem] flex-1 rounded-lg border border-reef bg-lagoon px-2 py-1 font-mono text-[11px]"
                                placeholder="a git URL, or owner/repo"
                                value={url}
                                onChange={(event) => set_url(event.target.value)}
                            />
                            <button
                                className="rounded-lg border border-reef px-3 py-1 font-mono text-[11px] text-shell hover:border-foam"
                                onClick={async () => {
                                    const picked = await pick_folder("Clone into…", into || undefined);
                                    if (picked) {
                                        set_into(picked);
                                    }
                                }}
                            >
                                clone into…
                            </button>
                        </div>
                        {url.trim() && into.trim() ? (
                            <p className="font-mono text-[10px] text-shade">
                                lands in {clone_target(as_url(url), into.trim())}
                            </p>
                        ) : null}
                    </div>
                )}
            </section>

            {where === "new" ? (
                <section className="flex flex-col gap-2 rounded-lg border border-reef bg-lagoon-deep p-2">
                    <h3 className="font-mono text-[9px] uppercase tracking-[0.14em] text-shade">
                        What to build it out of
                        {starters === null ? (
                            <Spinner
                                label="asking npm and cargo for today's versions"
                                className="ml-1.5 text-turquoise"
                            />
                        ) : null}
                    </h3>

                    {starters === null ? (
                        <Waiting
                            says="asking npm and cargo what these are today…"
                            className="font-mono text-[11px] text-shade"
                        />
                    ) : (
                        <div className="flex flex-col gap-2">
                            {starters.map((starter) => {
                                const picked = starter.id === stack;
                                return (
                                    <button
                                        key={starter.id}
                                        className={`flex min-w-0 flex-col gap-1 rounded-lg border px-2 py-1.5 text-left ${
                                            picked
                                                ? "border-turquoise"
                                                : starter.installed
                                                  ? "border-reef hover:border-foam"
                                                  : "border-reef opacity-60"
                                        }`}
                                        disabled={!starter.installed}
                                        onClick={() => {
                                            set_stack(picked ? null : starter.id);
                                            set_extras([]);
                                        }}
                                    >
                                        <span className="flex flex-wrap items-baseline gap-2">
                                            <span
                                                className={`font-mono text-[11px] ${picked ? "text-turquoise" : "text-shell"}`}
                                            >
                                                {starter.label}
                                            </span>
                                            {starter.version ? (
                                                <span className="font-mono text-[10px] text-palm">
                                                    {starter.version} today
                                                </span>
                                            ) : starter.installed ? (
                                                <span className="font-mono text-[10px] text-shade">
                                                    resolved at install time
                                                </span>
                                            ) : null}
                                        </span>
                                        <span className="font-mono text-[10px] text-driftwood">
                                            {starter.what}
                                        </span>
                                        {picked ? (
                                            <>
                                                <span className="font-mono text-[10px] text-shade">
                                                    {starter.why}
                                                </span>
                                                <span className="flex min-w-0 flex-col gap-1 rounded-md border border-reef bg-lagoon px-1.5 py-1">
                                                    {starter.commands.map((line) => (
                                                        <code
                                                            key={line}
                                                            className="whitespace-pre-wrap break-words font-mono text-[10px] leading-relaxed text-shell"
                                                        >
                                                            $ {line}
                                                        </code>
                                                    ))}
                                                </span>
                                                <span
                                                    className={`font-mono text-[10px] ${starter.audit_installed ? "text-palm" : "text-sun"}`}
                                                >
                                                    {starter.audit_installed
                                                        ? `${starter.audit} runs on it before the crew touches it`
                                                        : `${starter.audit} is not installed, so nothing will be checked`}
                                                </span>
                                                {starter.extras.map((held) => {
                                                    const on = extras.includes(held.id);
                                                    return (
                                                        <span
                                                            key={held.id}
                                                            className={`flex min-w-0 cursor-pointer flex-col gap-1 rounded-md border px-1.5 py-1 ${on ? "border-turquoise" : "border-reef"}`}
                                                            role="checkbox"
                                                            aria-checked={on}
                                                            tabIndex={0}
                                                            onClick={(event) => {
                                                                event.stopPropagation();
                                                                toggle_extra(held.id);
                                                            }}
                                                            onKeyDown={(event) => {
                                                                if (event.key !== "Enter" && event.key !== " ") {
                                                                    return;
                                                                }
                                                                event.preventDefault();
                                                                event.stopPropagation();
                                                                toggle_extra(held.id);
                                                            }}
                                                        >
                                                            <span className="flex flex-wrap items-baseline gap-2">
                                                                <span
                                                                    className={`font-mono text-[10px] ${on ? "text-turquoise" : "text-shell"}`}
                                                                >
                                                                    {on ? "✓" : "+"} {held.label}
                                                                </span>
                                                                {held.version ? (
                                                                    <span className="font-mono text-[10px] text-palm">
                                                                        {held.version} today
                                                                    </span>
                                                                ) : null}
                                                            </span>
                                                            <span className="font-mono text-[10px] text-driftwood">
                                                                {held.what}
                                                            </span>
                                                            {on ? (
                                                                <>
                                                                    <span className="font-mono text-[10px] text-shade">
                                                                        {held.why}
                                                                    </span>
                                                                    {held.commands.map((line) => (
                                                                        <code
                                                                            key={line}
                                                                            className="whitespace-pre-wrap break-words font-mono text-[10px] leading-relaxed text-shell"
                                                                        >
                                                                            $ {line}
                                                                        </code>
                                                                    ))}
                                                                    <span className="font-mono text-[10px] text-shade">
                                                                        {held.env
                                                                            .filter(([, generated]) => generated)
                                                                            .map(([key]) => key)
                                                                            .join(", ")}{" "}
                                                                        generated into {held.env_file}, which stays out
                                                                        of git
                                                                        {held.env.some(([, generated]) => !generated)
                                                                            ? `; ${held.env
                                                                                  .filter(([, generated]) => !generated)
                                                                                  .map(([key]) => key)
                                                                                  .join(", ")} left for you to fill in`
                                                                            : ""}
                                                                    </span>
                                                                </>
                                                            ) : null}
                                                        </span>
                                                    );
                                                })}
                                            </>
                                        ) : null}
                                        {starter.installed ? null : (
                                            <span className="font-mono text-[10px] text-sun">
                                                needs {starter.missing.join(" and ")} on PATH
                                            </span>
                                        )}
                                    </button>
                                );
                            })}
                        </div>
                    )}
                </section>
            ) : null}

            <section className="flex flex-col gap-2 rounded-lg border border-reef bg-lagoon-deep p-2">
                <h3 className="font-mono text-[9px] uppercase tracking-[0.14em] text-shade">
                    What the crew should do
                </h3>
                <textarea
                    className="min-h-[5rem] rounded-lg border border-reef bg-lagoon px-2 py-1 font-mono text-[11px]"
                    placeholder="an outcome, not a task — the commander takes it apart into steps"
                    value={goal}
                    onChange={(event) => set_goal(event.target.value)}
                />
                <p className="font-mono text-[10px] text-shade">
                    This becomes the commander's first brief. It plans and delegates; the agents it
                    hands steps to do the editing, each in its own worktree.
                </p>
            </section>

            <section className="flex flex-col gap-2 rounded-lg border border-reef bg-lagoon-deep p-2">
                <button
                    className="self-start font-mono text-[9px] uppercase tracking-[0.14em] text-shade hover:text-shell"
                    onClick={() => set_showing_more((held) => !held)}
                >
                    {showing_more ? "− names and engine" : "+ names and engine"}
                </button>

                {showing_more ? (
                    <div className="flex flex-col gap-2">
                        <div className="flex flex-wrap items-center gap-2">
                            <input
                                className="min-w-[10rem] flex-1 rounded-lg border border-reef bg-lagoon px-2 py-1 font-mono text-[11px]"
                                placeholder="workspace — the one you are in"
                                value={workspace}
                                onChange={(event) => set_workspace(event.target.value)}
                            />
                            <input
                                className="min-w-[10rem] flex-1 rounded-lg border border-reef bg-lagoon px-2 py-1 font-mono text-[11px]"
                                placeholder="worktree — named after the goal"
                                value={worktree}
                                onChange={(event) => set_worktree(event.target.value)}
                            />
                        </div>
                        <div className="flex flex-wrap items-center gap-2">
                            <input
                                className="min-w-[10rem] flex-1 rounded-lg border border-reef bg-lagoon px-2 py-1 font-mono text-[11px]"
                                placeholder="commander — X"
                                value={commander}
                                onChange={(event) => set_commander(event.target.value)}
                            />
                            <select
                                className="min-w-[10rem] flex-1 rounded-lg border border-reef bg-lagoon px-2 py-1 font-mono text-[11px]"
                                value={engine_id}
                                onChange={(event) => set_engine(event.target.value)}
                            >
                                <option value="">engine — whichever takes the crew's tools</option>
                                {engines.map((engine) => (
                                    <option key={engine.id} value={engine.id}>
                                        {engine.name}
                                    </option>
                                ))}
                            </select>
                        </div>
                    </div>
                ) : null}
            </section>

            {needs_git ? (
                <div className="flex flex-wrap items-center gap-2 rounded-lg border border-sun px-2 py-1">
                    <span className="font-mono text-[11px] text-sun">
                        {path.trim()} is not a git repository yet. Each agent works in its own
                        worktree, which needs one.
                    </span>
                    <button
                        className="rounded-lg border border-sun px-2 py-[3px] font-mono text-[11px] text-sun hover:bg-sun/10"
                        disabled={busy}
                        onClick={() => start(true)}
                    >
                        start one here and go
                    </button>
                </div>
            ) : null}

            {engines.length === 0 ? (
                <p className="font-mono text-[10px] text-sun">
                    No coding agent is on PATH. Install one — Claude Code, Codex, Gemini — and the
                    commander has something to run on.
                </p>
            ) : null}

            {error ? <p className="font-mono text-[11px] text-coral">{error}</p> : null}

            <button
                className="self-start rounded-lg border border-turquoise px-3 py-1 font-mono text-[11px] text-turquoise disabled:opacity-40"
                disabled={busy || !ready}
                onClick={() => start(false)}
            >
                {busy ? (
                    <Waiting says={where === "new" ? "making it, installing, auditing…" : "starting…"} />
                ) : (
                    "start"
                )}
            </button>
        </div>
    );
}
