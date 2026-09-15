/// Work a person started and is waiting on.
///
/// A button that asks the core for something looked exactly the same while the
/// core was working as it did before it was pressed, so a person pressed it
/// again and the core did the thing twice. Three pieces answer that, and they
/// live here because none of them belongs to any one button: a control that
/// will not take a second press while its first is still running, a count of
/// changes in flight that the whole window can show, and the same change asked
/// for again while the first is on its way, answered by the first.

/// Changes that come by the dozen and must each go through: keystrokes into a
/// pane, a pane being resized, a benchmark's samples, a pen's marks as they are
/// drawn. Folding two identical keystrokes into one would eat a letter, and
/// counting them would keep the bar lit for as long as anyone types.
const EVERY_ONE_COUNTS = [/\/sessions\/[^/]+\/input$/, /\/sessions\/[^/]+\/resize$/, /^\/metrics$/, /\/marks$/];

/// Whether a call to the core changes something a person would wait on.
export function is_a_change(path: string, init?: RequestInit): boolean {
    const method = (init?.method ?? "GET").toUpperCase();
    if (method === "GET" || method === "HEAD") {
        return false;
    }

    const bare = path.split("?")[0];
    return !EVERY_ONE_COUNTS.some((pattern) => pattern.test(bare));
}

/// What makes two changes the same change: the method, the path and the body.
/// A body that cannot be read back — a file on its way up — is never the same
/// as another, so it is never folded.
export function change_key(path: string, init?: RequestInit): string | null {
    if (!is_a_change(path, init)) {
        return null;
    }

    const body = init?.body;
    if (body !== undefined && body !== null && typeof body !== "string") {
        return null;
    }

    return `${(init?.method ?? "GET").toUpperCase()} ${path} ${body ?? ""}`;
}

let in_flight = 0;
const listeners = new Set<(count: number) => void>();
const on_their_way = new Map<string, Promise<unknown>>();

export function changes_in_flight(): number {
    return in_flight;
}

/// Be told whenever the number of changes in flight moves. Returns the way to
/// stop being told.
export function watch_changes(listener: (count: number) => void): () => void {
    listeners.add(listener);
    return () => {
        listeners.delete(listener);
    };
}

function tell(): void {
    for (const listener of listeners) {
        listener(in_flight);
    }
}

/// Send a change: counted while it runs, and folded into the same change when
/// one is already on its way.
export function send_change<T>(key: string | null, send: () => Promise<T>): Promise<T> {
    if (key !== null) {
        const held = on_their_way.get(key);
        if (held) {
            return held as Promise<T>;
        }
    }

    in_flight += 1;
    tell();

    const started: Promise<T> = send().finally(() => {
        in_flight -= 1;
        tell();
        if (key !== null && on_their_way.get(key) === started) {
            on_their_way.delete(key);
        }
    });

    if (key !== null) {
        on_their_way.set(key, started);
    }
    return started;
}

/// One control's presses. A press while the last one is still running is not a
/// second press: it is the same person, still waiting. The work starts inside
/// the press itself, so whatever it reads off the click is still there.
export function one_press_at_a_time() {
    let running: Promise<unknown> | null = null;

    return {
        busy: () => running !== null,
        press<T>(action: () => T | Promise<T>): Promise<T> | null {
            if (running) {
                return null;
            }

            const started = new Promise<T>((resolve) => resolve(action()));
            running = started;
            const done = () => {
                if (running === started) {
                    running = null;
                }
            };
            started.then(done, done);
            return started;
        },
    };
}
