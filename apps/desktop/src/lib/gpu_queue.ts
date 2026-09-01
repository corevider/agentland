/// Opening a terminal is not free: measured on this machine, xterm's own
/// `open()` costs about 45 ms and the first `fit()` another 28, so a page of
/// eight panes spent 600 ms in one block and the button felt dead. Heavy work
/// goes through here instead — one piece per step, so the layout appears at once
/// and the terminals fill in behind it.
export const STEP_MS = 16;

type Task = () => void;

const waiting: Task[] = [];
let timer: number | null = null;

function step(): void {
    timer = null;

    const task = waiting.shift();
    if (task) {
        task();
    }

    if (waiting.length > 0) {
        timer = window.setTimeout(step, STEP_MS);
    }
}

/// Queue the upgrade, and hand back the way to call it off — a pane that closes
/// before its turn must not be given a context nobody will dispose.
export function upgrade_soon(task: Task): () => void {
    waiting.push(task);

    if (timer === null) {
        timer = window.setTimeout(step, STEP_MS);
    }

    return () => {
        const at = waiting.indexOf(task);
        if (at >= 0) {
            waiting.splice(at, 1);
        }
    };
}

export function waiting_count(): number {
    return waiting.length;
}

/// For tests: forget everything queued and any timer waiting to run it.
export function reset_queue(): void {
    waiting.length = 0;
    if (timer !== null) {
        window.clearTimeout(timer);
        timer = null;
    }
}
