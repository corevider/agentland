const STORAGE_KEY = "agentland-settings";

/// How a terminal draws its cells. `webgl` is fastest under load; `dom` is
/// xterm's own renderer, slowest under load and the most direct on a webview
/// that shows a WebGL canvas only when the page next paints. `auto` takes
/// WebGL where a context is given.
export type Renderer = "auto" | "webgl" | "dom";

export const RENDERERS: Renderer[] = ["auto", "webgl", "dom"];

export interface Settings {
    panes: number;
    lines_per_second: number;
    duration_ms: number;
    renderer: Renderer;
}

export const DEFAULT_SETTINGS: Settings = {
    panes: 8,
    lines_per_second: 10_000,
    duration_ms: 30_000,
    renderer: "auto",
};

/// Fired on the window when settings are saved, so a pane already open can
/// follow a change without being rebuilt.
export const SETTINGS_EVENT = "agentland:settings";

export function load_settings(): Settings {
    try {
        const raw = localStorage.getItem(STORAGE_KEY);
        if (!raw) {
            return DEFAULT_SETTINGS;
        }
        return { ...DEFAULT_SETTINGS, ...(JSON.parse(raw) as Partial<Settings>) };
    } catch {
        return DEFAULT_SETTINGS;
    }
}

export function save_settings(settings: Settings): void {
    try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
    } catch {
        // storage can be unavailable; settings simply do not persist
    }
    window.dispatchEvent(new CustomEvent<Settings>(SETTINGS_EVENT, { detail: settings }));
}
