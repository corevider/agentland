import { useRef } from "react";
import { useFrame } from "@react-three/fiber";
import * as THREE from "three";

const FLIGHT_SECONDS = 1.1;
const ARC_HEIGHT = 2.6;

interface Props {
    from: [number, number, number];
    to: [number, number, number];
    color: string;
    on_done: () => void;
}

export function Projectile({ from, to, color, on_done }: Props) {
    const shell = useRef<THREE.Group>(null);
    const flash = useRef<THREE.Mesh>(null);
    const elapsed = useRef(0);
    const finished = useRef(false);

    useFrame((_, delta) => {
        if (finished.current || !shell.current) {
            return;
        }

        elapsed.current += delta;
        const progress = Math.min(elapsed.current / FLIGHT_SECONDS, 1);

        shell.current.position.set(
            from[0] + (to[0] - from[0]) * progress,
            from[1] + (to[1] - from[1]) * progress + Math.sin(progress * Math.PI) * ARC_HEIGHT,
            from[2] + (to[2] - from[2]) * progress,
        );

        const spin = elapsed.current * 6;
        shell.current.rotation.set(spin, spin * 0.7, 0);

        if (flash.current) {
            const material = flash.current.material as THREE.MeshBasicMaterial;
            const landing = Math.max(0, progress - 0.82) / 0.18;
            material.opacity = landing * 0.7;
            flash.current.scale.setScalar(0.4 + landing * 2.2);
        }

        if (progress >= 1) {
            finished.current = true;
            on_done();
        }
    });

    return (
        <group>
            <group ref={shell} position={from}>
                <mesh>
                    <icosahedronGeometry args={[0.17, 0]} />
                    <meshBasicMaterial color={color} />
                </mesh>
                <pointLight color={color} intensity={1.4} distance={4} />
            </group>

            <mesh ref={flash} position={[to[0], to[1] + 0.1, to[2]]} rotation={[-Math.PI / 2, 0, 0]}>
                <ringGeometry args={[0.32, 0.46, 12]} />
                <meshBasicMaterial color={color} transparent opacity={0} side={THREE.DoubleSide} />
            </mesh>
        </group>
    );
}
