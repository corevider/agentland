import { is_tauri } from "@/lib/core";

/// Which webview is rendering this window.
export function detect_surface(): string {
    const agent = navigator.userAgent;
    if (is_tauri()) {
        return agent.includes("WebKit") && !agent.includes("Chrome") ? "tauri-webkitgtk" : "tauri-webview";
    }
    if (agent.includes("Firefox")) {
        return "firefox";
    }
    if (agent.includes("Chrome") || agent.includes("Chromium")) {
        return "chromium";
    }
    return "webkit";
}
