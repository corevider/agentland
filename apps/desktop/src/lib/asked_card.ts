const ASKED = "agentland:card";

let asked: string | null = null;

/// Ask the board to open a card: from the jumper, or from a notice.
///
/// The ask is held until the board takes it, because the board is often not on
/// screen yet — its panel mounts when it comes forward, which is after the
/// asking. A board already on screen hears the ask at once.
export function ask_for_card(id: string): void {
    asked = id;
    window.dispatchEvent(new CustomEvent(ASKED));
}

/// The card that was asked for, once: taking it clears it.
export function take_asked_card(): string | null {
    const held = asked;
    asked = null;
    return held;
}

let asked_new = false;

/// Ask the board to start a new card, the way "+ new card" does.
export function ask_for_a_new_card(): void {
    asked_new = true;
    window.dispatchEvent(new CustomEvent(ASKED));
}

/// Whether a new card was asked for, once: taking it clears it.
export function take_new_card_ask(): boolean {
    const held = asked_new;
    asked_new = false;
    return held;
}

export function on_card_asked(listen: () => void): () => void {
    window.addEventListener(ASKED, listen);
    return () => window.removeEventListener(ASKED, listen);
}
