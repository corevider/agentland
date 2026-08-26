import { useCallback, useEffect, useRef, useState } from "react";

import { list_services, type Service } from "@/lib/core";

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

    useEffect(() => {
        if (!active) {
            return;
        }

        refresh().catch(() => undefined);
        const handle = window.setInterval(() => refresh().catch(() => undefined), 4000);
        return () => window.clearInterval(handle);
    }, [active, refresh]);

    const current = services.find((service) => service.key === selected) ?? null;

    return (
        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
            <div className="flex shrink-0 items-center gap-2 border-b border-reef/70 px-2 py-1.5">
                <select
                    className="min-w-0 max-w-[240px] rounded-lg border border-reef bg-lagoon-deep px-2 py-1 font-mono text-[10px]"
                    value={selected ?? ""}
                    onChange={(event) => set_selected(event.target.value || null)}
                >
                    {services.length === 0 ? <option value="">nothing is running</option> : null}
                    {services.map((service) => (
                        <option key={service.key} value={service.key}>
                            {service.repository_id}/{service.worktree} :{service.port}
                        </option>
                    ))}
                </select>

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
                <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-1 p-4 text-center">
                    <p className="font-mono text-[11px] text-shell">No dev server is running.</p>
                    <p className="font-mono text-[10px] text-shade">
                        Start one from the Repositories panel and it appears here, on its own port.
                    </p>
                </div>
            )}
        </div>
    );
}
