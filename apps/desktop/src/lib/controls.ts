/// Where a press lands on a control rather than on the thing around it.
///
/// A card is taken with the pointer, and a press that starts on its own
/// assign menu or its delete button is not somebody picking the card up. The
/// menu also takes the pointer with it: the release happens inside a popup the
/// window never hears about, so a watch armed on that press stays armed and
/// grabs the card on the next movement.
const CONTROLS = "button, select, input, textarea, label, [contenteditable]";

interface Pressed {
    closest?: (selector: string) => unknown;
}

export function on_a_control(target: Pressed | EventTarget | null): boolean {
    const pressed = target as Pressed | null;
    return typeof pressed?.closest === "function" && Boolean(pressed.closest(CONTROLS));
}

/// Keep the page from selecting text while something is carried across it.
///
/// A press that becomes a drag has already started a selection by the time
/// the drag begins, and every pane the pointer crosses adds to it. The
/// selection is dropped and the page told not to start another until the
/// returned function puts things back.
export function without_text_selection(): () => void {
    const body = document.body;
    const before = body.style.userSelect;
    body.style.userSelect = "none";
    window.getSelection()?.removeAllRanges();

    return () => {
        body.style.userSelect = before;
    };
}
