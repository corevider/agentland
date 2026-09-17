export const PROJECTS_CHANGED = "agentland:projects-changed";

export function changes_projects(path: string, method = "GET"): boolean {
    if (["GET", "HEAD", "OPTIONS"].includes(method.toUpperCase())) return false;
    return path === "/start" || path === "/repos" || path.startsWith("/repos/")
        || path === "/workspaces" || path.startsWith("/workspaces/");
}

let channel: BroadcastChannel | undefined;
function cross_window(): BroadcastChannel | undefined {
    if (typeof window === "undefined" || typeof BroadcastChannel === "undefined") return undefined;
    if (!channel) {
        try {
            channel = new BroadcastChannel("agentland-project-changes");
            channel.onmessage = () => window.dispatchEvent(new Event(PROJECTS_CHANGED));
        } catch {
            // Same-window events and polling still work where channels are unavailable.
            return undefined;
        }
    }
    return channel;
}

export function projects_changed(): void {
    if (typeof window === "undefined") return;
    window.dispatchEvent(new Event(PROJECTS_CHANGED));
    cross_window()?.postMessage(null);
}

export function watch_projects_changed(listener: () => void): () => void {
    cross_window();
    window.addEventListener(PROJECTS_CHANGED, listener);
    return () => window.removeEventListener(PROJECTS_CHANGED, listener);
}
