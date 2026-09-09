/// Where what you say goes.
///
/// Speaking used to end in one place: the pane on screen. So a person standing
/// in a box — a card's title, a brief, the goal a commander is about to be
/// given — held the button, said a sentence, and watched it land in a terminal
/// behind them. And the press itself took the focus away, so whatever had been
/// selected in that box was gone by the time they let go.
///
/// The box a person was in when they started speaking is where the words
/// belong. What is not a box — a terminal, the window itself — still goes to
/// the pane, which is what the button was for in the first place.

/// A field, read as three plain values so the rule can be tested without a DOM.
export interface Field {
    value: string;
    start: number;
    end: number;
}

/// What a node is, in the terms that decide whether it can be dictated into.
export interface Shape {
    tag: string;
    /// An input's type. A textarea has none, and an input written without one
    /// is a text input.
    type?: string | null;
    readonly?: boolean;
    disabled?: boolean;
    /// Inside a terminal. xterm keeps a hidden textarea to catch keystrokes,
    /// and writing into it types into nothing: a pane is written to through
    /// the core, which is the path that was already there.
    in_terminal?: boolean;
}

/// The input types that hold a sentence somebody would say.
///
/// A password is not one of them. Dictating into a password box is not a thing
/// people mean to do, and a spoken secret written quietly into one is worse
/// than a button that did nothing.
const SPOKEN_INTO = new Set(["", "text", "search", "url", "email", "tel"]);

export function takes_dictation(shape: Shape | null): boolean {
    if (!shape || shape.readonly || shape.disabled || shape.in_terminal) {
        return false;
    }

    const tag = shape.tag.toLowerCase();
    if (tag === "textarea") {
        return true;
    }

    return tag === "input" && SPOKEN_INTO.has((shape.type ?? "").toLowerCase());
}

/// What the field holds after a sentence is spoken into it, and where the
/// caret then sits.
///
/// Whatever was selected is replaced, the way typing would. A sentence dropped
/// against a word becomes one word — "fix the" and "auth" are not "fix theauth"
/// — so a single space goes on each side that needs one, and none where there
/// is already whitespace, a bracket, the punctuation that ends a sentence, or
/// nothing at all. The caret lands at the end of what was said rather than
/// after a space that was only put there to keep two words apart.
export function typed_into(field: Field, said: string): { value: string; caret: number } {
    const words = said.trim();
    const start = Math.max(0, Math.min(field.start, field.value.length));
    const end = Math.max(start, Math.min(field.end, field.value.length));

    if (words.length === 0) {
        return { value: field.value, caret: end };
    }

    const before = field.value.slice(0, start);
    const after = field.value.slice(end);
    const opens = before.length > 0 && !/[\s([{"'-]$/.test(before) ? " " : "";
    const closes = after.length > 0 && !/^[\s)\]},.;:!?]/.test(after) ? " " : "";

    return {
        value: `${before}${opens}${words}${closes}${after}`,
        caret: start + opens.length + words.length,
    };
}

/// Whether this node is inside a terminal rather than in a box of its own.
function in_terminal(node: Element): boolean {
    return Boolean(node.closest(".xterm"));
}

export function shape_of(node: Element | null): Shape | null {
    if (!node) {
        return null;
    }

    const held = node as HTMLInputElement & HTMLTextAreaElement;

    return {
        tag: node.tagName,
        type: node.getAttribute("type"),
        readonly: held.readOnly,
        disabled: held.disabled,
        in_terminal: in_terminal(node),
    };
}

/// Put the words in, the way a keystroke would.
///
/// React holds the value, so writing to `node.value` and stopping leaves the
/// box showing something the panel above it does not know about — and the next
/// render puts the old text back. The native setter plus an `input` event is
/// how a change from outside is made to look like one from inside.
export function speak_into(node: Element, said: string): boolean {
    if (!takes_dictation(shape_of(node))) {
        return false;
    }

    const held = node as HTMLInputElement | HTMLTextAreaElement;
    const { value, caret } = typed_into(
        { value: held.value, start: held.selectionStart ?? held.value.length, end: held.selectionEnd ?? held.value.length },
        said,
    );

    const setter = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(held), "value")?.set;
    if (setter) {
        setter.call(held, value);
    } else {
        held.value = value;
    }

    held.dispatchEvent(new Event("input", { bubbles: true }));
    held.focus();
    held.setSelectionRange(caret, caret);

    return true;
}
