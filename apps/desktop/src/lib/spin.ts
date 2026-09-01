/// The frames a person here already reads as "working".
///
/// The same braille cycle the engines' own spinners use, which `ReadablePane`
/// already knows to strip out of a transcript. Borrowing it rather than drawing
/// a circle keeps one vocabulary for the same idea.
export const SPINNER_FRAMES = [..."⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"];

export function spin_frame(tick: number): string {
    const count = SPINNER_FRAMES.length;
    return SPINNER_FRAMES[((tick % count) + count) % count];
}
