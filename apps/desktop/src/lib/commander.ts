/// Whether the commander's pane is worth clearing.
///
/// A commander that has seen a plan through carries the whole of it in its
/// context, and the next goal pays for every token of it. Once nothing it
/// held is still open, /clear costs nothing: every brief that follows is
/// composed again with its identity, the crew, what the project remembers
/// and its mail, and the house rules ride on a flag that a clear leaves alone.
export interface CommanderLoad {
    has_pane: boolean;
    running_plans: number;
    open_cards: number;
    finished_anything: boolean;
}

export function clear_is_recommended(load: CommanderLoad): boolean {
    return load.has_pane && load.running_plans === 0 && load.open_cards === 0 && load.finished_anything;
}
