const STORAGE_KEY = "agentland-settings";

export interface Settings {
    panes: number;
    lines_per_second: number;
    duration_ms: number;
}

export const DEFAULT_SETTINGS: Settings = {
    panes: 8,
    lines_per_second: 10_000,
    duration_ms: 30_000,
};

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
}
