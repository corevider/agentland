/// Choosing one of a list by typing at it.
///
/// A native dropdown answers typing with one letter and a jump to the next
/// thing that starts with it, which is no help at all once a list is a crew, a
/// project's worktrees, or every model an engine offers. This is the matching
/// behind a list that narrows as somebody types.

/// One thing that can be picked.
export interface Choice {
    value: string;
    label: string;
    /// The quieter half of the row — where an agent lives, what a login is
    /// signed in as. Searched, and shown, but never the reason a row is first.
    hint?: string;
    disabled?: boolean;
}

/// The size at which a list is worth typing at.
///
/// Below it a filter box is furniture: six rows are read faster than a word is
/// typed, and the box would take the keyboard away from the arrow keys.
export const WORTH_SEARCHING = 8;

/// How well one term lands on a choice. Zero means it does not.
///
/// The same order the jumper uses, because it is the order people expect: what
/// they typed exactly, then what starts with it, then what merely holds it, and
/// the quiet half of the row last of all.
function hit(label: string, hint: string, term: string): number {
    if (label === term) {
        return 100;
    }
    if (label.startsWith(term)) {
        return 80;
    }
    if (label.includes(term)) {
        return 60;
    }
    if (hint.includes(term)) {
        return 30;
    }

    return 0;
}

/// How well a choice answers what was typed, averaged over the words in it.
///
/// Every word has to land somewhere, because people type the way they think:
/// "ada agentland" is a person and then where they are, and neither half is a
/// substring of the other.
export function matches(choice: Choice, query: string): number {
    const terms = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
    if (terms.length === 0) {
        return 1;
    }

    const label = choice.label.toLowerCase();
    const hint = (choice.hint ?? "").toLowerCase();

    let total = 0;
    for (const term of terms) {
        const found = hit(label, hint, term);
        if (found === 0) {
            return 0;
        }
        total += found;
    }

    return total / terms.length;
}

/// The choices that answer what was typed, best first.
///
/// With nothing typed the list is left exactly as it was given: an order
/// somebody chose — the crew as it was hired, the models cheapest first — is
/// worth more than any sort this could invent.
export function narrow(choices: Choice[], query: string): Choice[] {
    if (query.trim().length === 0) {
        return choices;
    }

    return choices
        .map((choice, at) => ({ choice, at, hit: matches(choice, query) }))
        .filter((held) => held.hit > 0)
        .sort((left, right) => right.hit - left.hit || left.at - right.at)
        .map((held) => held.choice);
}

/// Where the highlight goes when the arrow keys are used.
///
/// It stops at both ends rather than wrapping: a list that jumps from the last
/// row back to the first is a list somebody scrolls past their answer in. A
/// choice that cannot be picked is stepped over rather than landed on.
export function step(choices: Choice[], from: number, by: number): number {
    if (choices.length === 0) {
        return -1;
    }

    let at = from;
    for (let taken = 0; taken < choices.length; taken += 1) {
        const next = at + by;
        if (next < 0 || next >= choices.length) {
            return at >= 0 && at < choices.length && !choices[at].disabled ? at : first_pickable(choices);
        }

        at = next;
        if (!choices[at].disabled) {
            return at;
        }
    }

    return from;
}

export function first_pickable(choices: Choice[]): number {
    const at = choices.findIndex((choice) => !choice.disabled);
    return at;
}

/// What the closed box shows: the label of what is picked, or the placeholder
/// when what is picked is not in the list any more — an agent let go, a model
/// an engine stopped offering.
export function shown(choices: Choice[], value: string, placeholder: string): string {
    return choices.find((choice) => choice.value === value)?.label ?? placeholder;
}
