import { describe, expect, it } from "vitest";

import {
    GRASS_SHARE,
    LIGHTHOUSE_ANGLE,
    palm_positions,
    seat_crew,
    station_placements,
    surface_height,
    terrace_layers,
    tier_for,
} from "@/island/geometry";

const radius = tier_for(7).radius;
const seats = station_placements(7, radius);

function angle_of(seat: { x: number; z: number }): number {
    return Math.atan2(seat.z, seat.x);
}

function apart(a: number, b: number): number {
    const difference = Math.abs(a - b) % (Math.PI * 2);
    return Math.min(difference, Math.PI * 2 - difference);
}

const crew = [
    { id: "ada", role: "implementer" },
    { id: "kai", role: "implementer" },
    { id: "nova", role: "ops" },
    { id: "rex", role: "reviewer" },
    { id: "ro", role: "implementer" },
    { id: "x", role: "commander" },
    { id: "zen", role: "implementer" },
];

describe("who stands where on the island", () => {
    it("puts the commander at the station nearest its own lighthouse", () => {
        const seated = seat_crew(crew, seats);
        const x = seated[crew.findIndex((member) => member.role === "commander")];

        const distances = seats.map((seat) => apart(angle_of(seat), LIGHTHOUSE_ANGLE));
        expect(apart(angle_of(x), LIGHTHOUSE_ANGLE)).toBeCloseTo(Math.min(...distances));
    });

    it("leaves everyone standing somewhere of their own", () => {
        const seated = seat_crew(crew, seats);
        expect(seated).toHaveLength(crew.length);
        expect(new Set(seated.map((seat) => `${seat.x},${seat.z}`)).size).toBe(crew.length);
    });

    it("changes nothing for a crew with no commander", () => {
        const plain = crew.filter((member) => member.role !== "commander");
        expect(seat_crew(plain, station_placements(plain.length, radius))).toEqual(
            station_placements(plain.length, radius),
        );
    });

    it("has nothing to swap when one agent is alone", () => {
        const alone = [{ id: "x", role: "commander" }];
        expect(seat_crew(alone, station_placements(1, radius))).toHaveLength(1);
    });
});

describe("the shape of the island", () => {
    const tier = tier_for(7);
    const layers = terrace_layers(tier, "a-seed");

    it("lays sand first and grass on top of it", () => {
        expect(layers[0].ground).toBe("sand");
        expect(layers.slice(1).every((layer) => layer.ground === "grass")).toBe(true);
        expect(layers[0].radius).toBeGreaterThan(layers[1].radius);
    });

    it("stands the crew on the sand, outside the grass", () => {
        const grass = tier.radius * GRASS_SHARE;
        for (const seat of station_placements(7, tier.radius)) {
            expect(Math.hypot(seat.x, seat.z)).toBeGreaterThan(grass);
            expect(Math.hypot(seat.x, seat.z)).toBeLessThan(tier.radius);
        }
    });

    it("gives the crew ground under their feet rather than a shelf edge", () => {
        const seat = station_placements(7, tier.radius)[0];
        const under_foot = surface_height(layers, Math.hypot(seat.x, seat.z));
        const in_the_middle = surface_height(layers, 0);

        expect(under_foot).toBeGreaterThan(0);
        expect(in_the_middle).toBeGreaterThan(under_foot);
    });

    it("keeps the trees in the green middle", () => {
        const grass = tier.radius * GRASS_SHARE;
        for (const palm of palm_positions(tier, "a-seed", station_placements(7, tier.radius))) {
            expect(Math.hypot(palm.x, palm.z)).toBeLessThan(grass);
        }
    });
});
