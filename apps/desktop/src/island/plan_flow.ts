import { type StationPlacement } from "@/island/geometry";

export interface FlowStep {
    id: string;
    title: string;
    state: string;
    needs: string[];
    assignee?: string | null;
}

export interface Marker {
    id: string;
    title: string;
    state: string;
    x: number;
    z: number;
    /// How high its label floats. Neighbouring steps take turns at three heights,
    /// so a row of them does not stack up on screen.
    lift: number;
    /// Where the work is being done, when someone has it.
    station: { x: number; z: number } | null;
}

export interface Thread {
    from: { x: number; z: number };
    to: { x: number; z: number };
    kind: "waits_for" | "handed_to";
}

/// A step's colour says what is true of it, not what would look nice: waiting is
/// unlit, assigned burns, done is green, blocked is red.
export const STEP_COLOR: Record<string, string> = {
    waiting: "#5d7d85",
    assigned: "#e0c05a",
    done: "#6fbf7d",
    blocked: "#e0705a",
};

export function color_of(state: string): string {
    return STEP_COLOR[state] ?? STEP_COLOR.waiting;
}

const ARC = Math.PI * 0.85;

/// Where the plan stands. The lighthouse is X's desk, but it sits on the far
/// side of the island from the camera's opening view, and a plan drawn behind
/// the crew is a plan nobody reads. So it stands on the near shoulder of the
/// island — between the lighthouse and the camera — where it is in front of the
/// crew and still on X's side of the scene.
const PLAN_ANGLE = Math.PI * 0.26;

/// Labels float at three heights in turn. Two were not enough once the arc
/// closes up in a narrow panel: neighbours still landed on one another.
const LIFTS = [0.7, 1.04, 1.38];

/// The plan stands in front of the lighthouse, because that is X's desk: one
/// marker per step, in the order the plan lists them, on an arc facing the crew.
export function markers_for(
    steps: FlowStep[],
    radius: number,
    stations: Map<string, StationPlacement>,
    spread_scale = 1,
): Marker[] {
    if (steps.length === 0) {
        return [];
    }

    const ring = radius * 0.5;
    const spread = Math.min(ARC, 0.46 * steps.length) * spread_scale;
    const start = PLAN_ANGLE - spread / 2;
    const step_angle = steps.length === 1 ? 0 : spread / (steps.length - 1);

    return steps.map((step, index) => {
        const angle = start + index * step_angle;
        const station = step.assignee ? stations.get(step.assignee) : undefined;

        return {
            id: step.id,
            title: step.title,
            state: step.state,
            x: Math.cos(angle) * ring,
            z: Math.sin(angle) * ring,
            lift: LIFTS[index % LIFTS.length],
            station: station ? { x: station.x, z: station.z } : null,
        };
    });
}

/// What the plan says about itself: which step waits for which, and which step
/// is in whose hands. Nothing is drawn for a dependency whose step is done —
/// a finished step is not waiting for anything any more.
export function threads_for(steps: FlowStep[], markers: Marker[]): Thread[] {
    const at = new Map(markers.map((marker) => [marker.id, marker]));
    const threads: Thread[] = [];

    for (const step of steps) {
        const marker = at.get(step.id);
        if (!marker) {
            continue;
        }

        if (step.state !== "done") {
            for (const needed of step.needs) {
                const earlier = at.get(needed);
                if (earlier) {
                    threads.push({ from: earlier, to: marker, kind: "waits_for" });
                }
            }
        }

        if (marker.station && step.state === "assigned") {
            threads.push({ from: marker, to: marker.station, kind: "handed_to" });
        }
    }

    return threads;
}

/// A plan is worth showing while it is being worked; a finished one leaves the
/// scene to the crew.
export function plan_to_show<T extends { state: string; steps: FlowStep[] }>(plans: T[]): T | null {
    return plans.find((plan) => plan.state === "running") ?? null;
}

/// How wide the plan may stand. A scene 260 px across cannot hold an arc that
/// reads well at 900; rather than letting the far steps fall off the edge, the
/// arc closes up as the panel narrows.
export function spread_for(canvas_width: number): number {
    if (canvas_width <= 0) {
        return 1;
    }

    return Math.min(1, Math.max(0.42, canvas_width / 620));
}
