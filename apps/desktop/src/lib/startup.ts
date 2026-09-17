import { read_machine } from "@/lib/core";

/** Probe through the webview's real authenticated HTTP path, not just a native
 * socket. Only this harmless read is retried; user actions are never replayed. */
export async function wait_for_core(signal: AbortSignal): Promise<void> {
    const deadline = Date.now() + 30_000;
    while (!signal.aborted) {
        const attempt = new AbortController();
        const cancel = () => attempt.abort();
        signal.addEventListener("abort", cancel, { once: true });
        const timeout = setTimeout(cancel, Math.min(2000, deadline - Date.now()));
        try {
            await read_machine(attempt.signal);
            signal.throwIfAborted();
            return;
        } catch (cause) {
            signal.throwIfAborted();
            if (Date.now() >= deadline) {
                throw new Error("Agentland could not connect to its local service. Try again without closing the window.", { cause });
            }
        } finally {
            clearTimeout(timeout);
            signal.removeEventListener("abort", cancel);
        }
        await new Promise<void>((resolve) => {
            const finish = () => {
                clearTimeout(timer);
                signal.removeEventListener("abort", finish);
                resolve();
            };
            const timer = setTimeout(finish, 250);
            signal.addEventListener("abort", finish, { once: true });
        });
    }
    signal.throwIfAborted();
}
