/// What arrives with a paste or a drop, as far as files go.
///
/// The browser's DataTransfer is awkward to build by hand, so the readers take
/// only the parts they use: a list of items that may each hold a file, and a
/// list of files. Both shapes are what a paste and a drop actually carry.
export interface Carried {
    items?: ArrayLike<{ kind: string; type: string; getAsFile(): File | null }>;
    files?: ArrayLike<File>;
}

const IMAGE_EXTENSION: Record<string, string> = {
    "image/png": "png",
    "image/jpeg": "jpg",
    "image/gif": "gif",
    "image/webp": "webp",
    "image/bmp": "bmp",
    "image/svg+xml": "svg",
};

function two(value: number): string {
    return String(value).padStart(2, "0");
}

/// A name for a picture that arrived without one.
///
/// A screenshot pasted from the clipboard is called "image.png" by every
/// browser, and a card with three of those is a card with three files that
/// cannot be told apart. The moment it was pasted can.
export function stamped_name(kind: string, at: Date = new Date()): string {
    const extension = IMAGE_EXTENSION[kind] ?? (kind.split("/")[1] || "bin");
    const stamp = `${at.getFullYear()}${two(at.getMonth() + 1)}${two(at.getDate())}-${two(at.getHours())}${two(at.getMinutes())}${two(at.getSeconds())}`;
    return `pasted-${stamp}.${extension}`;
}

/// Whether a name is one the browser made up rather than one a person chose.
export function is_a_placeholder_name(name: string): boolean {
    return /^(image|blob|file)?(\.[a-z0-9]+)?$/i.test(name.trim());
}

/// A file as it should be kept: named, if it came without a name.
function named(file: File, at: Date): File {
    if (!is_a_placeholder_name(file.name)) {
        return file;
    }
    return new File([file], stamped_name(file.type, at), { type: file.type });
}

/// The files in what was pasted, named where the browser did not.
///
/// A paste of text carries no files and gets an empty list, so the caller can
/// let the browser paste the text as it normally would.
export function files_from_paste(carried: Carried | null, at: Date = new Date()): File[] {
    if (!carried) {
        return [];
    }

    const out: File[] = [];
    const items = carried.items ?? [];
    for (let index = 0; index < items.length; index += 1) {
        const item = items[index];
        if (item.kind !== "file") {
            continue;
        }
        const file = item.getAsFile();
        if (file) {
            out.push(named(file, at));
        }
    }

    if (out.length === 0 && carried.files) {
        for (let index = 0; index < carried.files.length; index += 1) {
            out.push(named(carried.files[index], at));
        }
    }

    return out;
}

/// The files in what was dropped.
export function files_from_drop(carried: Carried | null, at: Date = new Date()): File[] {
    if (!carried?.files) {
        return [];
    }

    const out: File[] = [];
    for (let index = 0; index < carried.files.length; index += 1) {
        out.push(named(carried.files[index], at));
    }
    return out;
}

export function is_image(kind: string): boolean {
    return kind.startsWith("image/");
}

/// Whether a keyboard paste landed somewhere that will take it itself.
///
/// A paste into a text field is the field's to handle; only a paste that
/// lands nowhere in particular is the board's.
export function is_typing_into(target: EventTarget | null): boolean {
    if (!(target instanceof HTMLElement)) {
        return false;
    }
    return (
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        target.isContentEditable
    );
}
