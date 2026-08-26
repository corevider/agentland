import { useEffect, useMemo, useRef } from "react";
import { Canvas, useFrame, useThree } from "@react-three/fiber";
import * as THREE from "three";

import type { Agent } from "@/lib/core";
import {
    ROLE_SHAPE,
    STATE_COLOR,
    seeded_random,
    station_placements,
    tier_for,
    type Tier,
} from "@/island/geometry";

const FOCUSED_FPS = 30;
const BACKGROUND_FPS = 5;

function Terrain({ tier, seed }: { tier: Tier; seed: string }) {
    const layers = useMemo(() => {
        const random = seeded_random(seed);
        return Array.from({ length: tier.terraces }, (_, index) => {
            const shrink = index / (tier.terraces + 1);
            return {
                radius: tier.radius * (1 - shrink * 0.55),
                height: 0.34 + random() * 0.2,
                y: index * 0.3,
                rotation: random() * Math.PI,
            };
        });
    }, [tier, seed]);

    return (
        <group>
            {layers.map((layer, index) => (
                <mesh key={index} position={[0, layer.y, 0]} rotation={[0, layer.rotation, 0]} castShadow receiveShadow>
                    <cylinderGeometry args={[layer.radius * 0.86, layer.radius, layer.height, 7]} />
                    <meshLambertMaterial color={index === 0 ? "#c8b184" : "#3f6b4a"} flatShading />
                </mesh>
            ))}
        </group>
    );
}

function Palms({ tier, seed }: { tier: Tier; seed: string }) {
    const palms = useMemo(() => {
        const random = seeded_random(`${seed}-palms`);
        return Array.from({ length: tier.palms }, () => {
            const angle = random() * Math.PI * 2;
            const distance = tier.radius * (0.62 + random() * 0.3);
            return {
                x: Math.cos(angle) * distance,
                z: Math.sin(angle) * distance,
                height: 0.7 + random() * 0.5,
                tilt: (random() - 0.5) * 0.3,
            };
        });
    }, [tier, seed]);

    return (
        <group>
            {palms.map((palm, index) => (
                <group key={index} position={[palm.x, 0.2, palm.z]} rotation={[palm.tilt, 0, palm.tilt]}>
                    <mesh position={[0, palm.height / 2, 0]}>
                        <cylinderGeometry args={[0.04, 0.07, palm.height, 5]} />
                        <meshLambertMaterial color="#6b4b2f" flatShading />
                    </mesh>
                    <mesh position={[0, palm.height, 0]}>
                        <icosahedronGeometry args={[0.3, 0]} />
                        <meshLambertMaterial color="#2f7d52" flatShading />
                    </mesh>
                </group>
            ))}
        </group>
    );
}

function Station({
    agent,
    position,
    rotation,
    highlighted,
}: {
    agent: Agent;
    position: [number, number, number];
    rotation: number;
    highlighted: boolean;
}) {
    const shape = ROLE_SHAPE[agent.role] ?? "hut";
    const color = STATE_COLOR[agent.state] ?? STATE_COLOR.offline;
    const smoke = useRef<THREE.Mesh>(null);

    useFrame(({ clock }) => {
        if (smoke.current && agent.state === "working") {
            const time = clock.getElapsedTime();
            smoke.current.position.y = 0.9 + ((time * 0.4) % 0.7);
            const material = smoke.current.material as THREE.MeshLambertMaterial;
            material.opacity = 0.5 - ((time * 0.4) % 0.7) * 0.6;
        }
    });

    return (
        <group position={position} rotation={[0, rotation, 0]} userData={{ agent_id: agent.id }}>
            <mesh position={[0, 0.25, 0]} castShadow userData={{ agent_id: agent.id }}>
                <boxGeometry args={[0.62, 0.5, 0.62]} />
                <meshLambertMaterial color={highlighted ? "#45bcc4" : "#8a6a4a"} flatShading />
            </mesh>

            {shape === "watchtower" ? (
                <mesh position={[0, 0.95, 0]} castShadow userData={{ agent_id: agent.id }}>
                    <cylinderGeometry args={[0.16, 0.22, 0.9, 6]} />
                    <meshLambertMaterial color="#9a7d5c" flatShading />
                </mesh>
            ) : null}

            {shape === "crane" ? (
                <mesh position={[0.3, 0.85, 0]} rotation={[0, 0, -0.5]} castShadow userData={{ agent_id: agent.id }}>
                    <boxGeometry args={[0.9, 0.08, 0.08]} />
                    <meshLambertMaterial color="#b4541e" flatShading />
                </mesh>
            ) : null}

            {shape === "radio" ? (
                <mesh position={[0, 1.0, 0]} castShadow userData={{ agent_id: agent.id }}>
                    <coneGeometry args={[0.16, 0.7, 5]} />
                    <meshLambertMaterial color="#9aa7ad" flatShading />
                </mesh>
            ) : null}

            {shape === "workbench" ? (
                <mesh position={[0.36, 0.2, 0]} castShadow userData={{ agent_id: agent.id }}>
                    <boxGeometry args={[0.3, 0.12, 0.5]} />
                    <meshLambertMaterial color="#6b4b2f" flatShading />
                </mesh>
            ) : null}

            <mesh position={[0, 0.56, 0]}>
                <sphereGeometry args={[0.1, 8, 8]} />
                <meshBasicMaterial color={color} />
            </mesh>

            {agent.state === "working" ? (
                <mesh ref={smoke} position={[0, 0.9, 0]}>
                    <sphereGeometry args={[0.12, 6, 6]} />
                    <meshLambertMaterial color="#d8e2e6" transparent opacity={0.4} />
                </mesh>
            ) : null}
        </group>
    );
}

function Sea() {
    return (
        <mesh rotation={[-Math.PI / 2, 0, 0]} position={[0, -0.36, 0]} receiveShadow>
            <circleGeometry args={[26, 9]} />
            <meshLambertMaterial color="#123f4a" flatShading />
        </mesh>
    );
}

function Lighthouse({ radius, paused, highlighted }: { radius: number; paused: boolean; highlighted: boolean }) {
    return (
        <group position={[radius * 0.8, 0.2, -radius * 0.35]} userData={{ dispatch: true }}>
            <mesh position={[0, 0.7, 0]} castShadow userData={{ dispatch: true }}>
                <cylinderGeometry args={[0.16, 0.26, 1.4, 6]} />
                <meshLambertMaterial color={highlighted ? "#45bcc4" : "#d8e2e6"} flatShading />
            </mesh>
            <mesh position={[0, 1.5, 0]} userData={{ dispatch: true }}>
                <sphereGeometry args={[0.14, 8, 8]} />
                <meshBasicMaterial color={paused ? "#46565d" : "#e0c05a"} />
            </mesh>
        </group>
    );
}

function Jetty({ radius }: { radius: number }) {
    return (
        <mesh position={[-radius * 0.95, 0.05, 0]} castShadow>
            <boxGeometry args={[1.4, 0.1, 0.5]} />
            <meshLambertMaterial color="#7a5c3c" flatShading />
        </mesh>
    );
}

function Governor({ active }: { active: boolean }) {
    const { invalidate } = useThree();

    useEffect(() => {
        let handle = 0;

        const schedule = () => {
            const hidden = document.hidden;
            const fps = hidden ? 0 : active ? FOCUSED_FPS : BACKGROUND_FPS;
            if (fps > 0) {
                invalidate();
                handle = window.setTimeout(schedule, 1000 / fps);
            } else {
                handle = window.setTimeout(schedule, 500);
            }
        };

        schedule();
        return () => window.clearTimeout(handle);
    }, [active, invalidate]);

    return null;
}

function Orbit() {
    const { camera, gl, invalidate } = useThree();
    const state = useRef({ dragging: false, x: 0, angle: 0.9, pitch: 0.75 });

    useEffect(() => {
        const element = gl.domElement;
        const current = state.current;

        const place = () => {
            const distance = 14;
            camera.position.set(
                Math.cos(current.angle) * distance * Math.cos(current.pitch),
                Math.sin(current.pitch) * distance,
                Math.sin(current.angle) * distance * Math.cos(current.pitch),
            );
            camera.lookAt(0, 0.5, 0);
            invalidate();
        };

        const down = (event: PointerEvent) => {
            current.dragging = true;
            current.x = event.clientX;
        };
        const move = (event: PointerEvent) => {
            if (!current.dragging) {
                return;
            }
            current.angle += (event.clientX - current.x) * 0.008;
            current.x = event.clientX;
            place();
        };
        const up = () => {
            current.dragging = false;
        };

        place();
        element.addEventListener("pointerdown", down);
        window.addEventListener("pointermove", move);
        window.addEventListener("pointerup", up);

        return () => {
            element.removeEventListener("pointerdown", down);
            window.removeEventListener("pointermove", move);
            window.removeEventListener("pointerup", up);
        };
    }, [camera, gl, invalidate]);

    return null;
}

interface Props {
    agents: Agent[];
    seed: string;
    active: boolean;
    highlighted: string | null;
    paused: boolean;
    on_scene: (scene: THREE.Scene, camera: THREE.Camera) => void;
}

export function Island({ agents, seed, active, highlighted, paused, on_scene }: Props) {
    const tier = tier_for(agents.length);
    const placements = station_placements(agents.length, tier.radius);

    return (
        <Canvas
            frameloop="demand"
            shadows
            camera={{ position: [8, 8, 8], fov: 42 }}
            onCreated={({ scene, camera }) => on_scene(scene, camera)}
        >
            <color attach="background" args={["#0b1113"]} />
            <fog attach="fog" args={["#0b1113", 18, 34]} />
            <ambientLight intensity={0.55} />
            <directionalLight position={[6, 10, 4]} intensity={1.1} castShadow />

            <Governor active={active} />
            <Orbit />

            <Sea />
            <Terrain tier={tier} seed={seed} />
            <Palms tier={tier} seed={seed} />
            {tier.has_jetty ? <Jetty radius={tier.radius} /> : null}
            <Lighthouse
                radius={tier.radius}
                paused={paused}
                highlighted={highlighted === "__dispatch__"}
            />

            {agents.map((agent, index) => (
                <Station
                    key={agent.id}
                    agent={agent}
                    position={[placements[index].x, 0.3, placements[index].z]}
                    rotation={placements[index].rotation}
                    highlighted={highlighted === agent.id}
                />
            ))}
        </Canvas>
    );
}
