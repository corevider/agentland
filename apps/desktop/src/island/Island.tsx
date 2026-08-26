import { useEffect, useMemo, useRef } from "react";
import { Canvas, useFrame, useThree } from "@react-three/fiber";
import * as THREE from "three";

import type { Agent } from "@/lib/core";
import { Projectile } from "@/island/Projectile";
import { Robot } from "@/island/Robot";
import {
    JETTY_ANGLE,
    LIGHTHOUSE_ANGLE,
    PRESENCE_COLOR,
    ROLE_SHAPE,
    palm_positions,
    station_placements,
    surface_height,
    terrace_layers,
    tier_for,
    type StationPlacement,
    type Tier,
} from "@/island/geometry";

const FOCUSED_FPS = 30;
const BACKGROUND_FPS = 5;

function Terrain({ tier, seed }: { tier: Tier; seed: string }) {
    const layers = useMemo(() => terrace_layers(tier, seed), [tier, seed]);

    return (
        <group>
            {layers.map((layer, index) => (
                <mesh key={index} position={[0, layer.y, 0]} rotation={[0, layer.rotation, 0]} castShadow receiveShadow>
                    <cylinderGeometry args={[layer.radius * 0.86, layer.radius, layer.height, 7]} />
                    <meshLambertMaterial color={index === 0 ? "#e3cfa4" : "#4d7d55"} flatShading />
                </mesh>
            ))}
        </group>
    );
}

function Palms({
    tier,
    seed,
    stations,
}: {
    tier: Tier;
    seed: string;
    stations: StationPlacement[];
}) {
    const layers = useMemo(() => terrace_layers(tier, seed), [tier, seed]);
    const palms = useMemo(() => palm_positions(tier, seed, stations), [tier, seed, stations]);

    return (
        <group>
            {palms.map((palm, index) => (
                <group
                    key={index}
                    position={[palm.x, surface_height(layers, Math.hypot(palm.x, palm.z)), palm.z]}
                    rotation={[palm.tilt, 0, palm.tilt]}
                >
                    <mesh position={[0, palm.height / 2, 0]}>
                        <cylinderGeometry args={[0.04, 0.07, palm.height, 5]} />
                        <meshLambertMaterial color="#8a5f3c" flatShading />
                    </mesh>
                    <mesh position={[0, palm.height, 0]}>
                        <icosahedronGeometry args={[0.3, 0]} />
                        <meshLambertMaterial color="#3f9c63" flatShading />
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
    selected,
    on_select,
}: {
    agent: Agent;
    position: [number, number, number];
    rotation: number;
    highlighted: boolean;
    selected: boolean;
    on_select: (id: string) => void;
}) {
    const shape = ROLE_SHAPE[agent.role] ?? "hut";
    const presence = agent.presence ?? "idle";
    const color = PRESENCE_COLOR[presence] ?? PRESENCE_COLOR.idle;

    return (
        <group position={position} rotation={[0, rotation, 0]} userData={{ agent_id: agent.id }}>
            <mesh position={[0, 0.02, 0]} receiveShadow userData={{ agent_id: agent.id }}>
                <cylinderGeometry args={[0.44, 0.48, 0.06, 8]} />
                <meshLambertMaterial color={highlighted ? "#2b7f80" : "#c9b48c"} flatShading />
            </mesh>

            <Robot
                agent_id={agent.id}
                presence={presence}
                accent={color}
                highlighted={highlighted}
            />

            {shape === "watchtower" ? (
                <mesh position={[0.78, 0.5, -0.34]} castShadow userData={{ agent_id: agent.id }}>
                    <cylinderGeometry args={[0.1, 0.14, 1.0, 6]} />
                    <meshLambertMaterial color="#9a7d5c" flatShading />
                </mesh>
            ) : null}

            {shape === "crane" ? (
                <mesh position={[0.86, 0.55, -0.3]} rotation={[0, 0.4, -0.5]} castShadow userData={{ agent_id: agent.id }}>
                    <boxGeometry args={[0.8, 0.07, 0.07]} />
                    <meshLambertMaterial color="#b4541e" flatShading />
                </mesh>
            ) : null}

            {shape === "radio" ? (
                <mesh position={[0.76, 0.62, -0.3]} castShadow userData={{ agent_id: agent.id }}>
                    <coneGeometry args={[0.12, 0.7, 5]} />
                    <meshLambertMaterial color="#9aa7ad" flatShading />
                </mesh>
            ) : null}

            {shape === "workbench" ? (
                <mesh position={[0.74, 0.22, -0.28]} castShadow userData={{ agent_id: agent.id }}>
                    <boxGeometry args={[0.34, 0.1, 0.42]} />
                    <meshLambertMaterial color="#8a5f3c" flatShading />
                </mesh>
            ) : null}

            <mesh position={[0, 0.06, 0]}>
                <cylinderGeometry args={[0.3, 0.3, 0.01, 12]} />
                <meshBasicMaterial color={color} transparent opacity={0.5} />
            </mesh>
        </group>
    );
}

function Water({ radius }: { radius: number }) {
    return (
        <group>
            <mesh rotation={[-Math.PI / 2, 0, 0]} position={[0, -0.42, 0]} receiveShadow>
                <circleGeometry args={[220, 64]} />
                <meshLambertMaterial color="#0f3f4c" flatShading />
            </mesh>

            <mesh rotation={[-Math.PI / 2, 0, 0]} position={[0, -0.34, 0]}>
                <ringGeometry args={[radius * 0.92, radius * 2.4, 28]} />
                <meshLambertMaterial color="#1d7f84" flatShading transparent opacity={0.9} />
            </mesh>

            <mesh rotation={[-Math.PI / 2, 0, 0]} position={[0, -0.28, 0]}>
                <ringGeometry args={[radius * 0.9, radius * 1.35, 28]} />
                <meshLambertMaterial color="#3fb8ac" flatShading transparent opacity={0.85} />
            </mesh>

            <mesh rotation={[-Math.PI / 2, 0, 0]} position={[0, -0.22, 0]}>
                <ringGeometry args={[radius * 0.88, radius * 1.02, 28]} />
                <meshBasicMaterial color="#bfeee4" transparent opacity={0.55} />
            </mesh>
        </group>
    );
}

const SKY_VERTEX = `
varying vec3 world_direction;

void main() {
    vec4 world = modelMatrix * vec4(position, 1.0);
    world_direction = world.xyz;
    gl_Position = projectionMatrix * viewMatrix * world;
}
`;

const SKY_FRAGMENT = `
varying vec3 world_direction;

uniform vec3 zenith;
uniform vec3 upper;
uniform vec3 haze;
uniform vec3 band;
uniform vec3 below;

void main() {
    float height = normalize(world_direction).y;

    vec3 color;
    if (height >= 0.0) {
        vec3 low = mix(band, haze, smoothstep(0.0, 0.06, height));
        vec3 mid = mix(low, upper, smoothstep(0.03, 0.28, height));
        color = mix(mid, zenith, smoothstep(0.25, 0.75, height));
    } else {
        color = mix(band, below, smoothstep(0.0, 0.18, -height));
    }

    gl_FragColor = vec4(color, 1.0);
}
`;

function Sky() {
    const uniforms = useMemo(
        () => ({
            zenith: { value: new THREE.Color("#0a2634") },
            upper: { value: new THREE.Color("#1d5468") },
            haze: { value: new THREE.Color("#7fa79c") },
            band: { value: new THREE.Color("#f6c98d") },
            below: { value: new THREE.Color("#123a44") },
        }),
        [],
    );

    return (
        <mesh scale={[-1, 1, 1]}>
            <sphereGeometry args={[120, 32, 24]} />
            <shaderMaterial
                vertexShader={SKY_VERTEX}
                fragmentShader={SKY_FRAGMENT}
                uniforms={uniforms}
                side={THREE.BackSide}
                depthWrite={false}
                fog={false}
            />
        </mesh>
    );
}

function Lighthouse({
    radius,
    ground,
    paused,
    highlighted,
}: {
    radius: number;
    ground: number;
    paused: boolean;
    highlighted: boolean;
}) {
    return (
        <group
            position={[
                Math.cos(LIGHTHOUSE_ANGLE) * radius * 0.78,
                ground,
                Math.sin(LIGHTHOUSE_ANGLE) * radius * 0.78,
            ]}
            userData={{ dispatch: true }}
        >
            <mesh position={[0, 0.06, 0]} receiveShadow userData={{ dispatch: true }}>
                <cylinderGeometry args={[0.36, 0.44, 0.18, 7]} />
                <meshLambertMaterial color="#8c8375" flatShading />
            </mesh>
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
        <mesh
            position={[
                Math.cos(JETTY_ANGLE) * radius * 0.95,
                0.05,
                Math.sin(JETTY_ANGLE) * radius * 0.95,
            ]}
            castShadow
        >
            <boxGeometry args={[1.4, 0.1, 0.5]} />
            <meshLambertMaterial color="#a9764c" flatShading />
        </mesh>
    );
}

function LabelTracker({
    on_project,
}: {
    on_project: (marks: Array<{ id: string; x: number; y: number; visible: boolean }>) => void;
}) {
    const point = useRef(new THREE.Vector3());

    useFrame(({ scene, camera, size }) => {
        const marks: Array<{ id: string; x: number; y: number; visible: boolean }> = [];

        for (const node of scene.children) {
            const id = node.userData?.agent_id;
            if (typeof id !== "string") {
                continue;
            }

            node.getWorldPosition(point.current);
            point.current.y += 1.95;
            point.current.project(camera);

            marks.push({
                id,
                x: ((point.current.x + 1) / 2) * size.width,
                y: ((1 - point.current.y) / 2) * size.height,
                visible: point.current.z < 1,
            });
        }

        on_project(marks);
    });

    return null;
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
    const state = useRef({ dragging: false, x: 0, angle: 0.9, pitch: 0.42 });

    useEffect(() => {
        const element = gl.domElement;
        const current = state.current;

        const place = () => {
            const distance = 10.5;
            camera.position.set(
                Math.cos(current.angle) * distance * Math.cos(current.pitch),
                Math.sin(current.pitch) * distance,
                Math.sin(current.angle) * distance * Math.cos(current.pitch),
            );
            camera.lookAt(0, 0.9, 0);
            invalidate();
        };

        const down = (event: PointerEvent) => {
            current.dragging = true;
            current.x = event.clientX;
            element.setPointerCapture?.(event.pointerId);
        };

        const move = (event: PointerEvent) => {
            if (!current.dragging) {
                return;
            }

            if (event.buttons === 0) {
                current.dragging = false;
                return;
            }

            current.angle += (event.clientX - current.x) * 0.008;
            current.x = event.clientX;
            place();
        };

        const release = (event?: PointerEvent) => {
            current.dragging = false;
            if (event && element.hasPointerCapture?.(event.pointerId)) {
                element.releasePointerCapture(event.pointerId);
            }
        };

        place();
        element.addEventListener("pointerdown", down);
        element.addEventListener("pointerup", release);
        element.addEventListener("pointercancel", release);
        window.addEventListener("pointermove", move);
        window.addEventListener("pointerup", release);
        window.addEventListener("blur", () => release());

        return () => {
            element.removeEventListener("pointerdown", down);
            element.removeEventListener("pointerup", release);
            element.removeEventListener("pointercancel", release);
            window.removeEventListener("pointermove", move);
            window.removeEventListener("pointerup", release);
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
    shots: Array<{ seq: number; agent_id: string }>;
    on_project: (marks: Array<{ id: string; x: number; y: number; visible: boolean }>) => void;
    selected: string | null;
    on_select: (id: string) => void;
    on_shot_done: (seq: number) => void;
    on_scene: (scene: THREE.Scene, camera: THREE.Camera, invalidate: () => void) => void;
}

export function Island({
    agents,
    seed,
    active,
    highlighted,
    paused,
    shots,
    on_project,
    selected,
    on_select,
    on_shot_done,
    on_scene,
}: Props) {
    const tier = tier_for(agents.length);
    const placements = station_placements(agents.length, tier.radius);
    const layers = terrace_layers(tier, seed);
    const ground = surface_height(layers, tier.radius * 0.64);
    const lighthouse: [number, number, number] = [tier.radius * 0.78, ground + 1.7, -tier.radius * 0.3];

    return (
        <Canvas
            frameloop="demand"
            shadows
            gl={{ preserveDrawingBuffer: true }}
            camera={{ position: [8, 5, 8], fov: 46 }}
            onCreated={({ scene, camera, invalidate }) => on_scene(scene, camera, invalidate)}
        >
            <fog attach="fog" args={["#6f9b93", 30, 130]} />
            <ambientLight intensity={0.6} color="#bfe4e0" />
            <directionalLight position={[7, 9, 3]} intensity={1.15} color="#ffd9a8" castShadow />
            <hemisphereLight args={["#8fd3d0", "#2a4a3c", 0.45]} />

            <Governor active={active} />
            <LabelTracker on_project={on_project} />
            <Orbit />

            <Sky />
            <Water radius={tier.radius} />
            <Terrain tier={tier} seed={seed} />
            <Palms tier={tier} seed={seed} stations={placements} />
            {tier.has_jetty ? <Jetty radius={tier.radius} /> : null}
            <Lighthouse
                radius={tier.radius}
                ground={ground}
                paused={paused}
                highlighted={highlighted === "__dispatch__"}
            />

            {shots.map((shot) => {
                const index = agents.findIndex((agent) => agent.id === shot.agent_id);
                if (index < 0) {
                    return null;
                }

                return (
                    <Projectile
                        key={shot.seq}
                        from={lighthouse}
                        to={[placements[index].x, ground + 0.9, placements[index].z]}
                        color="#e0c05a"
                        on_done={() => on_shot_done(shot.seq)}
                    />
                );
            })}

            {agents.map((agent, index) => (
                <Station
                    key={agent.id}
                    agent={agent}
                    position={[placements[index].x, ground, placements[index].z]}
                    rotation={placements[index].rotation}
                    highlighted={highlighted === agent.id}
                    selected={selected === agent.id}
                    on_select={on_select}
                />
            ))}
        </Canvas>
    );
}
