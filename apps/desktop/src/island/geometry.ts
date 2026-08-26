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

export interface TerraceLayer {
    radius: number;
    height: number;
    y: number;
    rotation: number;
}

export function terrace_layers(tier: Tier, seed: string): TerraceLayer[] {
    const random = seeded_random(seed);

    return Array.from({ length: tier.terraces }, (_, index) => {
        const shrink = (index / (tier.terraces + 1)) * 0.55;
        return {
            radius: tier.radius * (1 - shrink),
            height: 0.34 + random() * 0.2,
            y: index * 0.3,
            rotation: random() * Math.PI,
        };
    });
}

export function surface_height(layers: TerraceLayer[], distance: number): number {
    let top = 0;

    for (const layer of layers) {
        const outer = layer.radius;
        if (distance <= outer * 0.94) {
            top = Math.max(top, layer.y + layer.height / 2);
        }
    }

    return top;
}

export interface StationPlacement {
    x: number;
    z: number;
    rotation: number;
}

export const LIGHTHOUSE_ANGLE = Math.atan2(-0.3, 0.78);
export const JETTY_ANGLE = Math.PI;

function angular_distance(a: number, b: number): number {
    const difference = Math.abs(a - b) % (Math.PI * 2);
    return Math.min(difference, Math.PI * 2 - difference);
}

export function station_placements(count: number, radius: number): StationPlacement[] {
    if (count === 0) {
        return [];
    }

    const ring_radius = radius * 0.64;
    const step = (Math.PI * 2) / count;
    const reserved = [LIGHTHOUSE_ANGLE, JETTY_ANGLE];

    let best_offset = 0;
    let best_clearance = -1;

    for (let sample = 0; sample < 180; sample += 1) {
        const offset = (sample / 180) * step;
        let clearance = Math.PI;

        for (let index = 0; index < count; index += 1) {
            const angle = offset + index * step;
            for (const taken of reserved) {
                clearance = Math.min(clearance, angular_distance(angle, taken));
            }
        }

        if (clearance > best_clearance) {
            best_clearance = clearance;
            best_offset = offset;
        }
    }

    return Array.from({ length: count }, (_, index) => {
        const angle = best_offset + index * step;
        return {
            x: Math.cos(angle) * ring_radius,
            z: Math.sin(angle) * ring_radius,
            rotation: -angle,
        };
    });
}

export function palm_positions(
    tier: Tier,
    seed: string,
    stations: StationPlacement[],
): Array<{ x: number; z: number; height: number; tilt: number }> {
    const random = seeded_random(`${seed}-palms`);
    const reserved = [
        ...stations.map((station) => ({ x: station.x, z: station.z })),
        {
            x: Math.cos(LIGHTHOUSE_ANGLE) * tier.radius * 0.78,
            z: Math.sin(LIGHTHOUSE_ANGLE) * tier.radius * 0.78,
        },
        { x: Math.cos(JETTY_ANGLE) * tier.radius * 0.95, z: Math.sin(JETTY_ANGLE) * tier.radius * 0.95 },
    ];

    const palms: Array<{ x: number; z: number; height: number; tilt: number }> = [];
    let attempts = 0;

    while (palms.length < tier.palms && attempts < tier.palms * 40) {
        attempts += 1;

        const angle = random() * Math.PI * 2;
        const inner = random() > 0.45;
        const distance = tier.radius * (inner ? 0.2 + random() * 0.22 : 0.8 + random() * 0.12);
        const x = Math.cos(angle) * distance;
        const z = Math.sin(angle) * distance;

        const clear = reserved.every((point) => Math.hypot(point.x - x, point.z - z) > 1.15);
        const spaced = palms.every((palm) => Math.hypot(palm.x - x, palm.z - z) > 0.8);

        if (clear && spaced) {
            palms.push({ x, z, height: 0.7 + random() * 0.5, tilt: (random() - 0.5) * 0.3 });
        }
    }

    return palms;
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
