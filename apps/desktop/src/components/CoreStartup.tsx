import { useEffect, useState, type ReactNode } from "react";
import { wait_for_core } from "@/lib/startup";

/** Panels must not interpret an unavailable service as an empty workspace. */
export function CoreStartup({ children }: { children: ReactNode }) {
    const [ready, set_ready] = useState(false);
    const [error, set_error] = useState<string | null>(null);
    const [attempt, set_attempt] = useState(0);

    useEffect(() => {
        const controller = new AbortController();
        set_error(null);
        wait_for_core(controller.signal).then(
            () => { if (!controller.signal.aborted) set_ready(true); },
            (cause: unknown) => {
                if (!controller.signal.aborted) set_error(cause instanceof Error ? cause.message : String(cause));
            },
        );
        return () => controller.abort();
    }, [attempt]);

    if (ready) return children;
    return (
        <main className="flex min-h-screen items-center justify-center bg-lagoon-deep p-8 text-linen">
            <section className="max-w-md space-y-4" aria-live="polite">
                <h1 className="font-display text-2xl">Agentland</h1>
                <p role={error ? "alert" : "status"}>
                    {error ?? "Connecting to the local service… Your workspace will open when it is ready."}
                </p>
                {error && <button className="rounded-shell border border-reef px-4 py-2" onClick={() => set_attempt((value) => value + 1)}>Try again</button>}
            </section>
        </main>
    );
}
