const STORAGE_KEY = "agentland-settings";

/// How a terminal draws its cells. `webgl` is fastest under load; `dom` is
/// xterm's own renderer, slowest under load and the most direct on a webview
/// that shows a WebGL canvas only when the page next paints. `auto` takes
/// WebGL where a context is given.
export type Renderer = "auto" | "webgl" | "dom";

export const RENDERERS: Renderer[] = ["auto", "webgl", "dom"];

/// What `auto` means on a given surface.
///
/// Measured on WebKitGTK without a GPU: a WebGL pane showed each keystroke
/// only when the next one made the page paint, and the DOM renderer showed
/// every one as it came. That matters for the pane somebody types into — the
/// commander's — so on WebKit that one draws with the DOM and the agents it
/// runs keep WebGL, which is faster under their output. Everywhere else
/// everything takes WebGL.
export function resolve_renderer(choice: Renderer, surface: string, typed_into = false): "webgl" | "dom" {
    if (choice !== "auto") {
        return choice;
    }

    return surface.includes("webkit") && typed_into ? "dom" : "webgl";
}

/// What one pane draws with, given its own footer choice and the settings'.
///
/// A chief's or a commander's pane is the one a person types into and glances
/// at while the crew works, so it opens on the DOM on every surface, whatever
/// the settings say for the rest. The pane's own footer still decides for it:
/// somebody who clicked it over to WebGL meant that pane.
export function pane_renderer(
    own: Renderer,
    settings: Renderer,
    surface: string,
    crowned: boolean,
): "webgl" | "dom" {
    if (own !== "auto") {
        return own;
    }

    return crowned ? "dom" : resolve_renderer(settings, surface, false);
}

export interface Settings {
    panes: number;
    lines_per_second: number;
    duration_ms: number;
    renderer: Renderer;
    /// Whether an agent's pane shows its login's five hours and week along its
    /// foot. Settings saved before there was a strip say nothing, which is on.
    pane_limits?: boolean;
}

export const DEFAULT_SETTINGS: Settings = {
    panes: 8,
    lines_per_second: 10_000,
    duration_ms: 30_000,
    renderer: "auto",
    pane_limits: true,
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
