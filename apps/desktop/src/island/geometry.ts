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
    ground: "sand" | "grass";
}

/// How far out the grass reaches. Beyond it the island is sand, and that is
/// where the crew stands: a station on a grass shelf with sand under its edge
/// reads as floating, and an island with its green in the middle and its people
/// on the shore reads as a place.
export const GRASS_SHARE = 0.6;
export const SAND_SHARE = 0.98;

/// The sand apron first, then the grass on top of it, terrace by terrace.
export function terrace_layers(tier: Tier, seed: string): TerraceLayer[] {
    const random = seeded_random(seed);

    const sand: TerraceLayer = {
        radius: tier.radius * SAND_SHARE,
        height: 0.36,
        y: 0,
        rotation: random() * Math.PI,
        ground: "sand",
    };

    const green = Array.from({ length: Math.max(1, tier.terraces - 1) }, (_, index) => {
        const shrink = (index / (tier.terraces + 1)) * 0.55;
        return {
            radius: tier.radius * GRASS_SHARE * (1 - shrink),
            height: 0.3 + random() * 0.18,
            y: 0.22 + index * 0.26,
            rotation: random() * Math.PI,
            ground: "grass" as const,
        };
    });

    return [sand, ...green];
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

    // The crew stands on the sand, outside the grass.
    const ring_radius = radius * 0.8;
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

        // Trees belong to the green middle, not to the shore the crew walks on.
        const angle = random() * Math.PI * 2;
        const distance = tier.radius * GRASS_SHARE * (0.22 + random() * 0.66);
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
    /// Alive at a prompt with nothing running: lit, but not busy.
    waiting: "#7fb8c4",
    attention: "#e5705f",
    idle: "#63797a",
};

export const PRESENCE_LABEL: Record<string, string> = {
    done: "finished",
    working: "working",
    waiting: "at a prompt",
    attention: "needs you",
    idle: "idle",
};

/// Who stands where.
///
/// The stations are evenly spaced, so which agent takes which one is otherwise
/// arbitrary — and the commander ending up on the far shore from the lighthouse
/// that stands for its own dispatching reads as two different people. X takes
/// the station nearest the lighthouse; the rest keep their order around the ring.
export function seat_crew<T extends { id: string; role?: string }>(
    crew: T[],
    placements: StationPlacement[],
): StationPlacement[] {
    const seats = placements.slice(0, crew.length);
    const commander = crew.findIndex((member) => member.role === "commander");

    if (commander < 0 || seats.length < 2) {
        return seats;
    }

    let nearest = 0;
    let closest = Infinity;

    for (let index = 0; index < seats.length; index += 1) {
        const angle = Math.atan2(seats[index].z, seats[index].x);
        const distance = angular_distance(angle, LIGHTHOUSE_ANGLE);
        if (distance < closest) {
            closest = distance;
            nearest = index;
        }
    }

    const seated = [...seats];
    [seated[commander], seated[nearest]] = [seated[nearest], seated[commander]];
    return seated;
}
