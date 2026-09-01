export interface Entry {
    name: string;
    kind: "dir" | "file";
    size: number;
}

/// Folders first, then files, each in the order a person reads them.
export function sort_entries(entries: Entry[]): Entry[] {
    return [...entries].sort((left, right) => {
        if (left.kind !== right.kind) {
            return left.kind === "dir" ? -1 : 1;
        }
        return left.name.localeCompare(right.name, undefined, { numeric: true, sensitivity: "base" });
    });
}

export function join_path(base: string, name: string): string {
    return base ? `${base}/${name}` : name;
}

export function parent_of(path: string): string {
    const cut = path.lastIndexOf("/");
    return cut < 0 ? "" : path.slice(0, cut);
}

/// The clickable trail above a listing: each crumb with the path it walks to.
export function crumbs_of(path: string): { name: string; path: string }[] {
    const crumbs = [{ name: "root", path: "" }];
    let walked = "";

    for (const part of path.split("/").filter(Boolean)) {
        walked = join_path(walked, part);
        crumbs.push({ name: part, path: walked });
    }

    return crumbs;
}

const BINARY = new Set([
    "png", "jpg", "jpeg", "gif", "webp", "ico", "pdf", "zip", "gz", "tar", "woff", "woff2",
    "ttf", "otf", "mp4", "mov", "wasm", "so", "dylib", "bin", "exe", "db", "sqlite",
]);

/// Whether opening this file will show text rather than a screenful of noise.
export function is_probably_text(name: string): boolean {
    const cut = name.lastIndexOf(".");
    if (cut <= 0) {
        return true;
    }

    return !BINARY.has(name.slice(cut + 1).toLowerCase());
}

export function size_word(bytes: number): string {
    if (bytes < 1024) {
        return `${bytes} B`;
    }
    if (bytes < 1024 * 1024) {
        return `${Math.round(bytes / 1024)} KB`;
    }
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export interface Hunk {
    file: string;
    lines: string[];
}

/// A patch split by the file it touches, so a long diff can be read one file at
/// a time instead of scrolled through as one wall.
export function hunks_of(patch: string): Hunk[] {
    const hunks: Hunk[] = [];
    let current: Hunk | null = null;

    for (const line of patch.split("\n")) {
        if (line.startsWith("diff --git ")) {
            const named = line.split(" b/").pop() ?? line;
            current = { file: named, lines: [] };
            hunks.push(current);
            continue;
        }

        if (current) {
            current.lines.push(line);
        }
    }

    return hunks;
}

export type LineKind = "added" | "removed" | "meta" | "same";

export function line_kind(line: string): LineKind {
    if (line.startsWith("+++") || line.startsWith("---") || line.startsWith("@@") || line.startsWith("index ")) {
        return "meta";
    }
    if (line.startsWith("+")) {
        return "added";
    }
    if (line.startsWith("-")) {
        return "removed";
    }
    return "same";
}
