import { use_poll } from "@/lib/poll";
import { useCallback, useEffect, useRef, useState } from "react";

import {
    list_agents,
    list_services,
    open_preview,
    photograph_pick,
    send_mail,
    type Agent,
    type PickPicture,
    type Service,
} from "@/lib/core";
import { design_note, path_of, recipients, type Pick } from "@/lib/design";
import { Picker } from "@/components/Picker";
import { Press } from "@/components/Press";

interface Props {
    active: boolean;
}

const BUTTON = "shrink-0 rounded-lg border px-2 py-1 font-mono text-[10px] disabled:opacity-40";

const message_of = (cause: unknown) => (cause instanceof Error ? cause.message : String(cause));

export function PreviewPanel({ active }: Props) {
    const [services, set_services] = useState<Service[]>([]);
    const [selected, set_selected] = useState<string | null>(null);
    const [nonce, set_nonce] = useState(0);
    const [error, set_error] = useState<string | null>(null);
    const frame = useRef<HTMLIFrameElement>(null);

    /// Design mode: the page as Agentland's preview serves it, which can point
    /// at an element. Held per dev server, and off for any other.
    const [designing, set_designing] = useState<{ key: string; url: string } | null>(null);
    const [picking, set_picking] = useState(false);
    const [pick, set_pick] = useState<Pick | null>(null);
    const [crew, set_crew] = useState<Agent[]>([]);
    const [to, set_to] = useState("");
    const [said, set_said] = useState("");
    const [sent, set_sent] = useState<string | null>(null);

    /// A picture of what was picked: being taken, taken, or why it could not be.
    const [shot, set_shot] = useState<"taking" | PickPicture | { error: string } | null>(null);
    const port_ref = useRef<number | null>(null);
    const turn_ref = useRef(0);
    // Ctrl+Enter sends as the button does, so both share one guard.
    const sending = useRef(false);

    // Taken the moment a pick arrives, from the dev server the pick came from.
    // A picture that comes back after another pick was made belongs to the
    // old one and is dropped.
    const take_picture = (picked: Pick) => {
        const port = port_ref.current;
        turn_ref.current += 1;
        const turn = turn_ref.current;
        if (port === null) {
            set_shot(null);
            return;
        }
        set_shot("taking");
        photograph_pick(port, {
            path: path_of(picked.url),
            box: picked.box,
            scroll: picked.scroll ?? { x: 0, y: 0 },
            viewport: picked.viewport,
        })
            .then((taken) => turn === turn_ref.current && set_shot(taken))
            .catch((cause) => turn === turn_ref.current && set_shot({ error: message_of(cause) }));
    };

    const refresh = useCallback(async () => {
        try {
            const running = await list_services();
            set_services(running);
            set_error(null);
            set_selected((current) =>
                current && running.some((service) => service.key === current)
                    ? current
                    : (running[0]?.key ?? null),
            );
        } catch (cause) {
            set_error(message_of(cause));
        }
    }, []);

    const current = services.find((service) => service.key === selected) ?? null;
    const design = designing && current && designing.key === current.key ? designing : null;
    port_ref.current = current?.port ?? null;

    // The crew is read with the dev servers while design mode is on, so a pick
    // can be addressed the moment it is made.
    const read_crew = useCallback(() => {
        list_agents()
            .then(set_crew)
            .catch(() => undefined);
    }, []);

    use_poll(() => {
        refresh().catch(() => undefined);
        if (design) {
            read_crew();
        }
    }, 4000, active);

    const ask_the_page = useCallback((on: boolean) => {
        frame.current?.contentWindow?.postMessage({ agentland: "pick", on }, "*");
    }, []);

    // The picker in the page says it is ready after every load, and says what
    // was picked, or that picking stopped. Only this frame is listened to.
    useEffect(() => {
        const heard = (event: MessageEvent) => {
            if (event.source !== frame.current?.contentWindow || !event.data || typeof event.data !== "object") {
                return;
            }
            const message = event.data as { agentland?: string; pick?: Pick };
            if (message.agentland === "ready" && picking) {
                ask_the_page(true);
            } else if (message.agentland === "picked" && message.pick) {
                set_picking(false);
                set_pick(message.pick);
                set_sent(null);
                take_picture(message.pick);
            } else if (message.agentland === "stopped") {
                set_picking(false);
            }
        };

        window.addEventListener("message", heard);
        return () => window.removeEventListener("message", heard);
    }, [picking, ask_the_page]);

    const switch_design = () => {
        if (!current) {
            return;
        }
        if (design) {
            set_designing(null);
            set_picking(false);
            set_pick(null);
            return;
        }
        return open_preview(current.port)
            .then(({ url }) => {
                set_designing({ key: current.key, url });
                set_error(null);
                read_crew();
            })
            .catch((cause) => set_error(message_of(cause)));
    };

    const start_picking = () => {
        set_pick(null);
        set_sent(null);
        set_picking(true);
        ask_the_page(true);
    };

    const stop_picking = () => {
        set_picking(false);
        ask_the_page(false);
    };

    // Whoever was chosen, while they are still on the list; otherwise whoever
    // works where the dev server runs.
    const choices = current ? recipients(current, crew) : [];
    const chosen = choices.some((agent) => agent.id === to) ? to : (choices[0]?.id ?? "");

    const send = () => {
        if (!pick || !current || !chosen || !said.trim() || sending.current) {
            return;
        }
        const picture = shot && typeof shot === "object" && "path" in shot ? shot.path : null;
        sending.current = true;
        return send_mail("a person", chosen, design_note(pick, said, current, picture))
            .then(() => {
                set_sent(`sent to ${chosen} · it reads this when its pane is quiet`);
                set_said("");
                set_pick(null);
            })
            .catch((cause) => set_error(message_of(cause)))
            .finally(() => {
                sending.current = false;
            });
    };

    return (
        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
            <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-reef/70 px-2 py-1.5">
                <Picker
                    className="min-w-0 max-w-[240px] rounded-lg border border-reef bg-lagoon-deep px-2 py-1 font-mono text-[10px]"
                    value={selected ?? ""}
                    placeholder={services.length === 0 ? "nothing is running" : "pick one"}
                    choices={services.map((service) => ({
                        value: service.key,
                        label: `${service.repository_id}/${service.worktree} :${service.port}`,
                    }))}
                    on_pick={(held) => set_selected(held || null)}
                />

                <span className="min-w-0 flex-1 truncate font-mono text-[10px] text-shade">
                    {current?.url ?? ""}
                </span>

                <Press
                    className={`${BUTTON} ${design ? "border-sun text-sun" : "border-foam"}`}
                    disabled={!current}
                    title="show the page through Agentland, where you can point at an element and hand it to an agent"
                    busy_says="opening the preview…"
                    on_press={switch_design}
                >
                    design mode {design ? "on" : "off"}
                </Press>

                {design ? (
                    picking ? (
                        <button className={`${BUTTON} border-sun text-sun`} onClick={stop_picking}>
                            picking · stop
                        </button>
                    ) : (
                        <button className={`${BUTTON} border-turquoise text-turquoise`} onClick={start_picking}>
                            pick an element
                        </button>
                    )
                ) : null}

                <button
                    className={`${BUTTON} border-foam`}
                    disabled={!current}
                    onClick={() => set_nonce((value) => value + 1)}
                >
                    reload
                </button>
            </div>

            {error ? (
                <div className="border-b border-coral px-2 py-1 font-mono text-[11px] text-coral">{error}</div>
            ) : null}

            {sent ? <div className="border-b border-reef px-2 py-1 font-mono text-[10px] text-palm">{sent}</div> : null}

            {pick && current ? (
                <section data-pick className="flex shrink-0 flex-col gap-1.5 border-b border-sun/60 bg-lagoon-deep px-2 py-1.5">
                    <div className="font-mono text-[10px] text-sun">
                        {pick.selector} · {pick.box.width}×{pick.box.height}
                    </div>
                    {pick.text ? (
                        <div className="truncate text-[11px] text-shell">“{pick.text.replace(/\s+/g, " ").slice(0, 160)}”</div>
                    ) : null}
                    <div data-pick-picture className="font-mono text-[10px] text-shade">
                        {shot === "taking" ? (
                            "taking a picture of it…"
                        ) : shot && "png" in shot ? (
                            <img
                                src={`data:image/png;base64,${shot.png}`}
                                alt="the picked element, rendered again"
                                className="max-h-24 max-w-full rounded border border-reef"
                            />
                        ) : shot && "error" in shot ? (
                            <span className="text-coral">no picture: {shot.error}</span>
                        ) : null}
                    </div>
                    <textarea
                        autoFocus
                        value={said}
                        onChange={(event) => set_said(event.target.value)}
                        onKeyDown={(event) => {
                            if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
                                event.preventDefault();
                                send();
                            }
                        }}
                        placeholder="what should change about it"
                        className="min-h-[3.5rem] rounded-md border border-reef bg-lagoon p-1.5 text-[11px] text-linen"
                    />
                    <div className="flex flex-wrap items-center gap-1.5">
                        {choices.length > 0 ? (
                            <Picker
                                className="min-w-0 max-w-[220px] rounded-lg border border-reef bg-lagoon px-2 py-1 font-mono text-[10px]"
                                value={chosen}
                                placeholder="who gets it"
                                choices={choices.map((agent) => ({
                                    value: agent.id,
                                    label: `${agent.name} · ${agent.worktree || agent.role}`,
                                }))}
                                on_pick={(held) => set_to(held)}
                            />
                        ) : (
                            <span className="font-mono text-[10px] text-coral">
                                nobody works on {current.repository_id} yet
                            </span>
                        )}
                        <Press
                            className={`${BUTTON} border-sun text-sun`}
                            disabled={!chosen || !said.trim()}
                            busy_says={`sending to ${chosen}…`}
                            on_press={send}
                        >
                            send to {chosen || "…"} · ctrl+enter
                        </Press>
                        <button className={`${BUTTON} border-reef text-shell`} onClick={() => set_pick(null)}>
                            discard
                        </button>
                    </div>
                </section>
            ) : null}

            {current ? (
                <iframe
                    ref={frame}
                    key={`${current.key}-${nonce}-${design ? "design" : "plain"}`}
                    src={design ? design.url : current.url}
                    title={`${current.repository_id}/${current.worktree}`}
                    className="min-h-0 min-w-0 flex-1 border-0 bg-white"
                />
            ) : (
                <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-1 p-2.5 text-center">
                    <p className="font-mono text-[11px] text-shell">No dev server is running.</p>
                    <p className="font-mono text-[10px] text-shade">
                        Start one from the Repositories panel and it appears here, on its own port.
                    </p>
                </div>
            )}
        </div>
    );
}
