export interface Tier {
    id: string;
    label: string;
    radius: number;
    terraces: number;
    palms: number;
    has_jetty: boolean;
    has_lighthouse: boolean;
}

const TIERS: Tier[] = [
    { id: "sandbar", label: "Sandbar", radius: 3.2, terraces: 1, palms: 2, has_jetty: false, has_lighthouse: false },
    { id: "beach", label: "Beach and palm grove", radius: 4.2, terraces: 2, palms: 5, has_jetty: true, has_lighthouse: false },
    { id: "forest", label: "Forest and ridge", radius: 5.2, terraces: 3, palms: 9, has_jetty: true, has_lighthouse: false },
    { id: "settlement", label: "Settlement", radius: 6.4, terraces: 4, palms: 14, has_jetty: true, has_lighthouse: true },
];

export function tier_for(crew_size: number): Tier {
    if (crew_size <= 3) {
        return TIERS[0];
    }
    if (crew_size <= 6) {
        return TIERS[1];
    }
    if (crew_size <= 10) {
        return TIERS[2];
    }
    return TIERS[3];
}

export interface StationPlacement {
    x: number;
    z: number;
    rotation: number;
}

export function station_placements(count: number, radius: number): StationPlacement[] {
    const ring_radius = radius * 0.58;
    return Array.from({ length: count }, (_, index) => {
        const angle = (index / Math.max(count, 1)) * Math.PI * 2 + Math.PI / 6;
        return {
            x: Math.cos(angle) * ring_radius,
            z: Math.sin(angle) * ring_radius,
            rotation: -angle,
        };
    });
}

export function seeded_random(seed: string): () => number {
    let value = 0;
    for (let index = 0; index < seed.length; index += 1) {
        value = (value * 31 + seed.charCodeAt(index)) >>> 0;
    }

    return () => {
        value = (value * 1664525 + 1013904223) >>> 0;
        return value / 0xffffffff;
    };
}

export const ROLE_SHAPE: Record<string, "workbench" | "watchtower" | "radio" | "crane" | "hut"> = {
    implementer: "workbench",
    reviewer: "watchtower",
    tester: "radio",
    researcher: "radio",
    ops: "crane",
};

export const PRESENCE_COLOR: Record<string, string> = {
    done: "#6fbf73",
    working: "#f0a95c",
    attention: "#e5705f",
    idle: "#63797a",
};

export const PRESENCE_LABEL: Record<string, string> = {
    done: "finished",
    working: "working",
    attention: "needs you",
    idle: "idle",
};
