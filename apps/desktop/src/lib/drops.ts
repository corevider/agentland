const PLAIN = /^[A-Za-z0-9_@%+=:,./-]+$/;

/// A path as a shell reads it: left alone when nothing in it would be split or
/// read as syntax, and single-quoted when something would.
export function quoted(path: string): string {
    return PLAIN.test(path) ? path : `'${path.replace(/'/g, `'\\''`)}'`;
}

/// Paths the way a terminal is handed them when files are dropped on it: each
/// quoted where it has to be, separated by spaces, and a space after the last
/// so whoever dropped them can go on typing the sentence they belong to.
export function as_typed(paths: string[]): string {
    return paths.length === 0 ? "" : `${paths.map(quoted).join(" ")} `;
}
