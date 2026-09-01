import { useMemo } from "react";
import { useThree } from "@react-three/fiber";
import * as THREE from "three";

import {
    color_of,
    markers_for,
    spread_for,
    threads_for,
    type FlowStep,
    type Marker,
    type Thread,
} from "@/island/plan_flow";
import type { StationPlacement } from "@/island/geometry";

/// A thread between two points on the ground: a thin box, turned to face along
/// the line and stretched to its length, which is cheaper than a tube and reads
/// better than a hairline at this camera distance.
function Line({ thread, ground }: { thread: Thread; ground: number }) {
    const { position, rotation, length } = useMemo(() => {
        const from = new THREE.Vector3(thread.from.x, 0, thread.from.z);
        const to = new THREE.Vector3(thread.to.x, 0, thread.to.z);
        const span = to.clone().sub(from);

        return {
            position: from.clone().add(span.clone().multiplyScalar(0.5)),
            rotation: Math.atan2(span.z, span.x),
            length: span.length(),
        };
    }, [thread]);

    const handed = thread.kind === "handed_to";

    return (
        <mesh
            position={[position.x, ground + (handed ? 0.09 : 0.06), position.z]}
            rotation={[0, -rotation, 0]}
        >
            <boxGeometry args={[length, 0.012, handed ? 0.05 : 0.03]} />
            <meshBasicMaterial
                color={handed ? "#e0c05a" : "#3f6b72"}
                transparent
                opacity={handed ? 0.85 : 0.5}
            />
        </mesh>
    );
}

function Step({ marker, ground }: { marker: Marker; ground: number }) {
    const color = color_of(marker.state);
    const done = marker.state === "done";

    return (
        <group position={[marker.x, ground, marker.z]} userData={{ step_id: marker.id, label_lift: marker.lift }}>
            <mesh position={[0, 0.03, 0]} receiveShadow>
                <cylinderGeometry args={[0.17, 0.2, 0.06, 6]} />
                <meshLambertMaterial color="#b9a67f" flatShading />
            </mesh>

            {/* A post carries the step: short and dark once it is done, standing
                and lit while it is someone's to do. */}
            <mesh position={[0, done ? 0.11 : 0.24, 0]} castShadow>
                <cylinderGeometry args={[0.05, 0.06, done ? 0.1 : 0.36, 5]} />
                <meshLambertMaterial color="#7d6a4f" flatShading />
            </mesh>

            <mesh position={[0, done ? 0.19 : 0.46, 0]}>
                <octahedronGeometry args={[done ? 0.08 : 0.11]} />
                <meshBasicMaterial color={color} />
            </mesh>
        </group>
    );
}

/// What X is running, drawn where X stands: one marker per step of the plan in
/// front of the lighthouse, a thread from a step to whatever it waits for, and a
/// thread from a step to the station of whoever is holding it.
export function PlanFlow({
    steps,
    radius,
    stations,
    ground,
}: {
    steps: FlowStep[];
    radius: number;
    stations: Map<string, StationPlacement>;
    ground: number;
}) {
    const width = useThree((state) => state.size.width);

    const { markers, threads } = useMemo(() => {
        const placed = markers_for(steps, radius, stations, spread_for(width));
        return { markers: placed, threads: threads_for(steps, placed) };
    }, [radius, stations, steps, width]);

    if (markers.length === 0) {
        return null;
    }

    return (
        <group>
            {threads.map((thread, index) => (
                <Line key={index} thread={thread} ground={ground} />
            ))}
            {markers.map((marker) => (
                <Step key={marker.id} marker={marker} ground={ground} />
            ))}
        </group>
    );
}
