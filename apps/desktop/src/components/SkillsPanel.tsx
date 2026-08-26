import { useCallback, useEffect, useMemo, useState } from "react";

import {
    agent_skills,
    install_skill,
    list_agents,
    list_skills,
    remove_skill,
    uninstall_skill,
    write_skill,
    type Agent,
    type Skill,
} from "@/lib/core";

const BLANK_MANIFEST = `---
name: Ship checklist
description: What to do before a release.
when_to_use: Cutting a release.
---
Run the tests. Read the diff. Tag it.
`;

interface Props {
    active: boolean;
}

export function SkillsPanel({ active }: Props) {
    const [skills, set_skills] = useState<Skill[]>([]);
    const [agents, set_agents] = useState<Agent[]>([]);
    const [installs, set_installs] = useState<Record<string, string[]>>({});
    const [selected, set_selected] = useState<string | null>(null);
    const [drafting, set_drafting] = useState(false);
    const [draft, set_draft] = useState(BLANK_MANIFEST);
    const [notice, set_notice] = useState<string | null>(null);

    const refresh = useCallback(async () => {
        const [library, crew] = await Promise.all([list_skills(), list_agents()]);
        set_skills(library);
        set_agents(crew);

        const pairs = await Promise.all(
            crew.map(async (agent) => [agent.id, (await agent_skills(agent.id)).map((s) => s.id)] as const),
        );
        set_installs(Object.fromEntries(pairs));
        set_selected((current) => current ?? library[0]?.id ?? null);
    }, []);

    useEffect(() => {
        if (!active) {
            return;
        }
        refresh().catch((cause) => set_notice(cause instanceof Error ? cause.message : String(cause)));
    }, [active, refresh]);

    const current = useMemo(
        () => skills.find((skill) => skill.id === selected) ?? null,
        [selected, skills],
    );

    const run = useCallback(
        async (action: () => Promise<unknown>) => {
            set_notice(null);
            try {
                await action();
                await refresh();
            } catch (cause) {
                set_notice(cause instanceof Error ? cause.message : String(cause));
            }
        },
        [refresh],
    );

    const toggle = useCallback(
        (agent: Agent, skill: Skill) => {
            const held = installs[agent.id]?.includes(skill.id) ?? false;
            void run(() =>
                held ? uninstall_skill(agent.id, skill.id) : install_skill(agent.id, skill.id),
            );
        },
        [installs, run],
    );

    const save_draft = useCallback(() => {
        const name = /name:\s*(.+)/.exec(draft)?.[1]?.trim() ?? "";
        if (!name) {
            set_notice("the manifest needs a name");
            return;
        }

        void run(async () => {
            const written = await write_skill(name, draft);
            set_selected(written.id);
            set_drafting(false);
            set_draft(BLANK_MANIFEST);
        });
    }, [draft, run]);

    return (
        <div className="flex min-h-0 min-w-0 flex-1">
            <div className="flex w-[220px] shrink-0 flex-col border-r border-reef/70">
                <div className="flex-1 overflow-y-auto p-2">
                    {skills.map((skill) => {
                        const holders = agents.filter((agent) =>
                            installs[agent.id]?.includes(skill.id),
                        ).length;

                        return (
                            <button
                                key={skill.id}
                                onClick={() => {
                                    set_selected(skill.id);
                                    set_drafting(false);
                                }}
                                className={`mb-1 w-full rounded-lg border px-2 py-2 text-left ${
                                    skill.id === selected && !drafting
                                        ? "border-turquoise bg-lagoon-deep"
                                        : "border-transparent hover:border-reef"
                                }`}
                            >
                                <div className="truncate text-xs text-linen">{skill.name}</div>
                                <div className="mt-0.5 flex items-center gap-2 font-mono text-[10px] text-shade">
                                    <span>{skill.builtin ? "built in" : "yours"}</span>
                                    {holders > 0 ? (
                                        <span className="text-palm">
                                            · {holders} {holders === 1 ? "agent" : "agents"}
                                        </span>
                                    ) : null}
                                </div>
                            </button>
                        );
                    })}
                </div>

                <button
                    className="m-2 shrink-0 rounded-lg border border-foam px-2 py-1 font-mono text-[11px]"
                    onClick={() => {
                        set_drafting(true);
                        set_notice(null);
                    }}
                >
                    write your own
                </button>
            </div>

            <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto p-3">
                {notice ? (
                    <div className="mb-2 rounded-lg border border-coral px-2 py-1 font-mono text-[11px] text-coral">
                        {notice}
                    </div>
                ) : null}

                {drafting ? (
                    <>
                        <p className="mb-2 font-mono text-[11px] text-shell">
                            A skill is a folder with a SKILL.md. This is that file.
                        </p>
                        <textarea
                            className="min-h-[220px] flex-1 rounded-lg border border-reef bg-lagoon-deep p-2 font-mono text-[11px] leading-relaxed text-driftwood"
                            value={draft}
                            spellCheck={false}
                            onChange={(event) => set_draft(event.target.value)}
                        />
                        <div className="mt-2 flex gap-2">
                            <button
                                className="rounded-lg border border-turquoise px-3 py-1 text-xs text-turquoise"
                                onClick={save_draft}
                            >
                                save it
                            </button>
                            <button
                                className="rounded-lg border border-foam px-3 py-1 text-xs"
                                onClick={() => set_drafting(false)}
                            >
                                cancel
                            </button>
                        </div>
                    </>
                ) : current ? (
                    <>
                        <div className="flex items-start justify-between gap-3">
                            <div className="min-w-0">
                                <div className="font-display text-lg text-linen">{current.name}</div>
                                <div className="mt-0.5 text-xs text-shell">{current.description}</div>
                                <div className="mt-0.5 font-mono text-[11px] text-driftwood">
                                    Use it when: {current.when_to_use}
                                </div>
                            </div>
                            {current.builtin ? null : (
                                <button
                                    className="shrink-0 rounded-lg border border-coral px-2 py-1 font-mono text-[10px] text-coral"
                                    onClick={() => run(() => remove_skill(current.id))}
                                >
                                    delete
                                </button>
                            )}
                        </div>

                        <section className="mt-3">
                            <h3 className="mb-1 font-mono text-[10px] uppercase tracking-[0.12em] text-shell">
                                Give it to
                            </h3>
                            {agents.length === 0 ? (
                                <p className="font-mono text-[11px] text-shade">Nobody is hired yet.</p>
                            ) : (
                                <div className="flex flex-wrap gap-2">
                                    {agents.map((agent) => {
                                        const held = installs[agent.id]?.includes(current.id) ?? false;
                                        return (
                                            <button
                                                key={agent.id}
                                                onClick={() => toggle(agent, current)}
                                                className={`rounded-lg border px-2 py-1 font-mono text-[11px] ${
                                                    held
                                                        ? "border-palm text-palm"
                                                        : "border-reef text-shell hover:border-foam"
                                                }`}
                                            >
                                                {held ? "✓ " : ""}
                                                {agent.name}
                                            </button>
                                        );
                                    })}
                                </div>
                            )}
                            <p className="mt-1 font-mono text-[10px] text-shade">
                                It joins their opening brief the next time they start.
                            </p>
                        </section>

                        <section className="mt-3 min-h-0">
                            <h3 className="mb-1 font-mono text-[10px] uppercase tracking-[0.12em] text-shell">
                                What it tells them
                            </h3>
                            <pre className="overflow-auto whitespace-pre-wrap rounded-lg border border-reef bg-lagoon-deep p-2 font-mono text-[11px] leading-relaxed text-driftwood">
                                {current.body}
                            </pre>
                        </section>
                    </>
                ) : (
                    <p className="font-mono text-[11px] text-shade">The library is empty.</p>
                )}
            </div>
        </div>
    );
}
