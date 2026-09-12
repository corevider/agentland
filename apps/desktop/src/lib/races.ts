import type { Agent, Engine, Entrant, Race, Task } from "@/lib/core";

export const FEWEST = 2;
export const MOST = 4;

/// Model names an engine's command line takes as they are. Only Claude's short
/// names are settled enough to offer; any other engine takes what is typed.
export const MODELS_FOR: Record<string, string[]> = {
    claude: ["opus", "sonnet", "haiku"],
};

export interface LaneDraft {
    engine_id: string;
    model: string;
}

/// The race still running on this card, if there is one.
export function race_on(races: Race[], task_id: string): Race | null {
    return races.find((race) => race.task_id === task_id && race.ended_at === null) ?? null;
}

/// Where a new race starts: two lanes that differ. Two engines when two are
/// installed, since that is the comparison most worth making; otherwise one
/// engine on two of its models.
export function first_lanes(engines: Engine[]): LaneDraft[] {
    const installed = engines.filter((engine) => engine.installed);
    if (installed.length >= 2) {
        return installed.slice(0, 2).map((engine) => ({
            engine_id: engine.id,
            model: MODELS_FOR[engine.id]?.[0] ?? "",
        }));
    }

    const only = installed[0];
    if (!only) {
        return [];
    }
    const models = MODELS_FOR[only.id] ?? [];
    return [
        { engine_id: only.id, model: models[0] ?? "" },
        { engine_id: only.id, model: models[1] ?? "" },
    ];
}

/// The next lane to add: an engine and model not in the race yet, when there
/// is one left to offer.
export function another_lane(lanes: LaneDraft[], engines: Engine[]): LaneDraft | null {
    const taken = new Set(lanes.map((lane) => `${lane.engine_id}/${lane.model}`));
    for (const engine of engines.filter((held) => held.installed)) {
        for (const model of MODELS_FOR[engine.id] ?? [""]) {
            if (!taken.has(`${engine.id}/${model}`)) {
                return { engine_id: engine.id, model };
            }
        }
    }
    return null;
}

/// Why a card cannot be raced, said on the card rather than refused after a
/// click. Null when it can be.
export function why_not_race(task: Task, running: Race | null): string | null {
    if (running) {
        return `already racing in ${running.id}`;
    }
    if (task.assignee) {
        return `${task.assignee} holds it`;
    }
    if (task.worktree) {
        return `it is bound to the ${task.worktree} worktree`;
    }
    if (task.column === "review" || task.column === "ready" || task.column === "done") {
        return `it is already in ${task.column}`;
    }
    return null;
}

/// "claude · opus", or the engine alone when the model is the engine's own.
export function lane_words(lane: { engine_id: string; model?: string | null }): string {
    const model = lane.model?.trim();
    return model ? `${lane.engine_id} · ${model}` : lane.engine_id;
}

/// How an entrant is doing, as far as the crew list says: gone once let go.
export function standing_of(entrant: Entrant, agents: Agent[]): string {
    return agents.find((agent) => agent.id === entrant.agent_id)?.presence ?? "gone";
}
