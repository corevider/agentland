/// A file somebody asked to see from outside the Files & Git panel: the
/// jumper, for now.
export interface AskedFile {
    repository_id: string;
    /// The worktree it is read from; null for the project's own checkout.
    worktree: string | null;
    /// Its path inside that checkout.
    path: string;
}

const ASKED = "agentland:file";

let asked: AskedFile | null = null;

/// Ask the Files & Git panel to open a file. Held until the panel takes it,
/// because the panel is often not on screen yet — it mounts when it comes
/// forward, which is after the asking. A panel already on screen hears the
/// ask at once.
export function ask_for_file(file: AskedFile): void {
    asked = file;
    window.dispatchEvent(new CustomEvent(ASKED));
}

/// The file that was asked for, once: taking it clears it.
export function take_asked_file(): AskedFile | null {
    const held = asked;
    asked = null;
    return held;
}

export function on_file_asked(listen: () => void): () => void {
    window.addEventListener(ASKED, listen);
    return () => window.removeEventListener(ASKED, listen);
}

/// The folder a file sits in, which is what the panel lists around it.
export function folder_of(path: string): string {
    const at = path.lastIndexOf("/");
    return at < 0 ? "" : path.slice(0, at);
}
