/// One line of a unified diff, and where it sits in the two files it compares.
export interface PatchLine {
    text: string;
    kind: "file" | "meta" | "hunk" | "added" | "removed" | "context";
    file: string | null;
    old_line: number | null;
    new_line: number | null;
}

/// A note a person pinned to one line of a diff.
export interface Note {
    file: string;
    line: number;
    /// Which file the line number counts in: the new one, or the old one for a
    /// line the change removed.
    side: "new" | "old";
    excerpt: string;
    text: string;
}

const HUNK = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/;

/// Read a patch into lines that know their file and their line numbers.
///
/// A header is only a header before the first hunk of its file: inside a hunk
/// a removed line that happened to start with "-- " reads "--- ", and taking
/// it for the next file's header put every note after it on the wrong file.
export function read_patch(patch: string): PatchLine[] {
    const lines: PatchLine[] = [];
    let file: string | null = null;
    let old_at = 0;
    let new_at = 0;
    let in_hunk = false;
    const plain = (text: string, kind: PatchLine["kind"]) =>
        lines.push({ text, kind, file, old_line: null, new_line: null });

    for (const text of patch.split("\n")) {
        if (text.startsWith("diff --git ")) {
            in_hunk = false;
            file = text.match(/ b\/(.+)$/)?.[1] ?? file;
            plain(text, "file");
            continue;
        }

        const hunk = text.match(HUNK);
        if (hunk) {
            old_at = Number(hunk[1]);
            new_at = Number(hunk[2]);
            in_hunk = true;
            plain(text, "hunk");
            continue;
        }

        if (!in_hunk) {
            if (text.startsWith("+++ ") && !text.includes("/dev/null")) {
                file = text.slice(4).trim().replace(/^b\//, "");
            }
            plain(text, "meta");
            continue;
        }

        if (text.startsWith("+")) {
            lines.push({ text, kind: "added", file, old_line: null, new_line: new_at });
            new_at += 1;
        } else if (text.startsWith("-")) {
            lines.push({ text, kind: "removed", file, old_line: old_at, new_line: null });
            old_at += 1;
        } else if (text.startsWith("\\")) {
            plain(text, "meta");
        } else {
            lines.push({ text, kind: "context", file, old_line: old_at, new_line: new_at });
            old_at += 1;
            new_at += 1;
        }
    }

    return lines;
}

/// Whether a note can be pinned to this line: only to code, never to a header.
export function pinnable(line: PatchLine): boolean {
    return line.file !== null && (line.kind === "added" || line.kind === "removed" || line.kind === "context");
}

/// Where a note on this line points.
export function anchor_of(line: PatchLine): Pick<Note, "file" | "line" | "side" | "excerpt"> {
    const removed = line.kind === "removed";
    return {
        file: line.file ?? "",
        line: (removed ? line.old_line : line.new_line) ?? 0,
        side: removed ? "old" : "new",
        excerpt: line.text.slice(1).trim().slice(0, 120),
    };
}

/// Every note as one request for changes.
///
/// Sent as a batch rather than one at a time: an agent handed notes one by one
/// swings back and forth between them, and handed all of them it revises once.
/// Ordered by file and line, each pinned to where it was left.
export function notes_as_review(notes: Note[]): string {
    const ordered = [...notes].sort((one, other) => one.file.localeCompare(other.file) || one.line - other.line);
    const said = ordered.map((note, index) => {
        const where = `${note.file}:${note.line}${note.side === "old" ? " (a removed line)" : ""}`;
        const text = note.text.trim().replace(/\n/g, "\n   ");
        return `${index + 1}. ${where} — \`${note.excerpt}\`\n   ${text}`;
    });

    const count = `${notes.length} note${notes.length === 1 ? "" : "s"}`;
    return [
        `${count} on the diff, each pinned to its line. Address every one, then commit and open the pull request again.`,
        ...said,
    ].join("\n\n");
}
