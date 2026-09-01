import { is_tauri } from "@/lib/core";

/// Ask the operating system for a folder.
///
/// Typing a path into a box is not how anyone opens a project — they point at
/// it. Outside the desktop app there is no picker to ask (a browser cannot see
/// the filesystem), so this answers null and the caller falls back to a typed
/// path rather than pretending the dialog was cancelled.
export async function pick_folder(title: string, start_at?: string): Promise<string | null> {
    if (!is_tauri()) {
        return null;
    }

    const { open } = await import("@tauri-apps/plugin-dialog");
    const chosen = await open({
        directory: true,
        multiple: false,
        title,
        defaultPath: start_at,
    });

    return typeof chosen === "string" ? chosen : null;
}

/// The folder a clone would land in, named after the repository in the URL.
///
/// Shown before the clone runs, because "clone into ~/code" and "clone into
/// ~/code/svc-demo" are different promises and only one of them is true.
export function clone_target(url: string, into: string): string | null {
    const trimmed = url.trim().replace(/\/+$/, "");
    if (!trimmed || !into) {
        return null;
    }

    const last = trimmed.split(/[/:]/).pop() ?? "";
    const name = last.replace(/\.git$/, "");
    if (!name) {
        return null;
    }

    return `${into.replace(/\/+$/, "")}/${name}`;
}

/// Whether this looks like something git can clone.
export function is_clonable(url: string): boolean {
    const trimmed = url.trim();

    return (
        /^(https?|git|ssh):\/\/\S+\/\S+/.test(trimmed) ||
        /^[\w.-]+@[\w.-]+:\S+\/\S+/.test(trimmed) ||
        /^[\w.-]+\/[\w.-]+$/.test(trimmed)
    );
}

/// What a bare "owner/repo" means. GitHub is the only host we can guess for,
/// and guessing wrong is better caught here than by git after a long pause.
export function as_url(entered: string): string {
    const trimmed = entered.trim();

    return /^[\w.-]+\/[\w.-]+$/.test(trimmed) ? `https://github.com/${trimmed}.git` : trimmed;
}
