import { useRef } from "react";
import { useFrame } from "@react-three/fiber";
import * as THREE from "three";

const PANEL = "#eef0e6";
const JOINT = "#37474a";
const VISOR = "#122024";

interface Props {
    agent_id: string;
    presence: string;
    accent: string;
    highlighted: boolean;
}

export function Robot({ agent_id, presence, accent, highlighted }: Props) {
    const body = useRef<THREE.Group>(null);
    const left_arm = useRef<THREE.Group>(null);
    const right_arm = useRef<THREE.Group>(null);
    const head = useRef<THREE.Group>(null);
    const bulb = useRef<THREE.Mesh>(null);

    useFrame(({ clock }) => {
        const time = clock.getElapsedTime();

        if (bulb.current) {
            const material = bulb.current.material as THREE.MeshBasicMaterial;
            material.opacity =
                presence === "attention"
                    ? 0.55 + Math.abs(Math.sin(time * 3.2)) * 0.45
                    : presence === "working"
                      ? 0.7 + Math.sin(time * 1.6) * 0.15
                      : presence === "done"
                        ? 0.9
                        : 0.25;
        }

        if (presence === "working") {
            const swing = Math.sin(time * 2.4) * 0.35;
            if (left_arm.current) {
                left_arm.current.rotation.x = swing;
            }
            if (right_arm.current) {
                right_arm.current.rotation.x = -swing;
            }
            if (body.current) {
                body.current.position.y = Math.sin(time * 2.4) * 0.015;
            }
            return;
        }

        if (presence === "attention") {
            const turn = Math.sin(time * 1.1) * 0.5;
            if (head.current) {
                head.current.rotation.y = turn;
            }
            if (left_arm.current) {
                left_arm.current.rotation.x = -0.9;
            }
            if (right_arm.current) {
                right_arm.current.rotation.x = 0;
            }
            return;
        }

        if (left_arm.current) {
            left_arm.current.rotation.x = 0;
        }
        if (right_arm.current) {
            right_arm.current.rotation.x = 0;
        }
        if (head.current) {
            head.current.rotation.y = 0;
        }
        if (body.current) {
            body.current.position.y = 0;
        }
    });

    const panel = highlighted ? "#8fe0d5" : PANEL;

    return (
        <group ref={body} userData={{ agent_id }} scale={0.44}>
            <group position={[0, 2.12, 0]} userData={{ agent_id }}>
                <mesh position={[0, -0.14, 0]} userData={{ agent_id }}>
                    <cylinderGeometry args={[0.015, 0.015, 0.16, 4]} />
                    <meshLambertMaterial color={JOINT} flatShading />
                </mesh>
                <mesh position={[0, -0.03, 0]} userData={{ agent_id }}>
                    <cylinderGeometry args={[0.07, 0.05, 0.05, 6]} />
                    <meshLambertMaterial color={JOINT} flatShading />
                </mesh>
                <mesh ref={bulb} position={[0, 0.08, 0]} userData={{ agent_id }}>
                    <icosahedronGeometry args={[0.12, 0]} />
                    <meshBasicMaterial color={accent} transparent opacity={0.8} />
                </mesh>
                <pointLight color={accent} intensity={presence === "idle" ? 0.15 : 0.6} distance={2.4} />
            </group>

            <group ref={head} position={[0, 1.62, 0]} userData={{ agent_id }}>
                <mesh castShadow userData={{ agent_id }}>
                    <boxGeometry args={[0.34, 0.3, 0.3]} />
                    <meshLambertMaterial color={panel} flatShading />
                </mesh>
                <mesh position={[0, -0.01, 0.155]} userData={{ agent_id }}>
                    <boxGeometry args={[0.3, 0.16, 0.02]} />
                    <meshBasicMaterial color={VISOR} />
                </mesh>
            </group>

            <mesh position={[0, 1.4, 0]} userData={{ agent_id }}>
                <cylinderGeometry args={[0.07, 0.07, 0.12, 6]} />
                <meshLambertMaterial color={JOINT} flatShading />
            </mesh>

            <mesh position={[0, 1.02, 0]} castShadow userData={{ agent_id }}>
                <boxGeometry args={[0.5, 0.62, 0.28]} />
                <meshLambertMaterial color={panel} flatShading />
            </mesh>

            <mesh position={[0, 0.66, 0]} userData={{ agent_id }}>
                <boxGeometry args={[0.34, 0.16, 0.24]} />
                <meshLambertMaterial color={JOINT} flatShading />
            </mesh>

            <group ref={left_arm} position={[-0.32, 1.28, 0]} userData={{ agent_id }}>
                <mesh position={[0, -0.28, 0]} castShadow userData={{ agent_id }}>
                    <boxGeometry args={[0.13, 0.58, 0.13]} />
                    <meshLambertMaterial color={panel} flatShading />
                </mesh>
                <mesh position={[0, -0.62, 0]} userData={{ agent_id }}>
                    <boxGeometry args={[0.11, 0.14, 0.11]} />
                    <meshLambertMaterial color={JOINT} flatShading />
                </mesh>
            </group>

            <group ref={right_arm} position={[0.32, 1.28, 0]} userData={{ agent_id }}>
                <mesh position={[0, -0.28, 0]} castShadow userData={{ agent_id }}>
                    <boxGeometry args={[0.13, 0.58, 0.13]} />
                    <meshLambertMaterial color={panel} flatShading />
                </mesh>
                <mesh position={[0, -0.62, 0]} userData={{ agent_id }}>
                    <boxGeometry args={[0.11, 0.14, 0.11]} />
                    <meshLambertMaterial color={JOINT} flatShading />
                </mesh>
            </group>

            <mesh position={[-0.14, 0.3, 0]} castShadow userData={{ agent_id }}>
                <boxGeometry args={[0.16, 0.58, 0.16]} />
                <meshLambertMaterial color={panel} flatShading />
            </mesh>
            <mesh position={[0.14, 0.3, 0]} castShadow userData={{ agent_id }}>
                <boxGeometry args={[0.16, 0.58, 0.16]} />
                <meshLambertMaterial color={panel} flatShading />
            </mesh>

            <mesh position={[-0.14, 0.03, 0.03]} userData={{ agent_id }}>
                <boxGeometry args={[0.18, 0.08, 0.26]} />
                <meshLambertMaterial color={JOINT} flatShading />
            </mesh>
            <mesh position={[0.14, 0.03, 0.03]} userData={{ agent_id }}>
                <boxGeometry args={[0.18, 0.08, 0.26]} />
                <meshLambertMaterial color={JOINT} flatShading />
            </mesh>
        </group>
    );
}
