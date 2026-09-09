import { use_poll } from "@/lib/poll";
import { useCallback, useEffect, useRef, useState } from "react";

import { list_services, type Service } from "@/lib/core";
import { Picker } from "@/components/Picker";

interface Props {
    active: boolean;
}

export function PreviewPanel({ active }: Props) {
    const [services, set_services] = useState<Service[]>([]);
    const [selected, set_selected] = useState<string | null>(null);
    const [nonce, set_nonce] = useState(0);
    const [error, set_error] = useState<string | null>(null);
    const frame = useRef<HTMLIFrameElement>(null);

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
            set_error(cause instanceof Error ? cause.message : String(cause));
        }
    }, []);

    use_poll(() => {
        refresh().catch(() => undefined);
    }, 4000, active);

    const current = services.find((service) => service.key === selected) ?? null;

    return (
        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
            <div className="flex shrink-0 items-center gap-2 border-b border-reef/70 px-2 py-1.5">
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

                <button
                    className="shrink-0 rounded-lg border border-foam px-2 py-1 font-mono text-[10px] disabled:opacity-40"
                    disabled={!current}
                    onClick={() => set_nonce((value) => value + 1)}
                >
                    reload
                </button>
            </div>

            {error ? (
                <div className="border-b border-coral px-2 py-1 font-mono text-[11px] text-coral">{error}</div>
            ) : null}

            {current ? (
                <iframe
                    ref={frame}
                    key={`${current.key}-${nonce}`}
                    src={current.url}
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
