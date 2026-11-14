import { useEffect, useMemo, useRef, useState } from 'react';
import * as THREE from 'three';
import { Canvas, useFrame } from '@react-three/fiber';
import { getSignalEngine } from '../lib/useSignal';
import { useAccentColor } from '../lib/useAccentColor';
import { SIGNAL_FRAG, SIGNAL_VERT } from './shaders/signal';

function useVisible<T extends HTMLElement>(ref: React.RefObject<T | null>) {
  const [visible, setVisible] = useState(true);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const io = new IntersectionObserver(([entry]) => setVisible(entry.isIntersecting));
    io.observe(el);
    return () => io.disconnect();
  }, [ref]);
  return visible;
}

const BANDS = 31;

function SignalScene() {
  const engine = getSignalEngine();
  const accent = useAccentColor();
  const floor = useRef<THREE.InstancedMesh>(null);
  const dummy = useMemo(() => new THREE.Object3D(), []);
  const tint = useMemo(() => new THREE.Color(), []);
  const uniforms = useMemo(
    () => ({
      uTime: { value: 0 },
      uLevel: { value: 0 },
      uAccent: { value: new THREE.Color('#ff2e57') },
    }),
    [],
  );

  useEffect(() => {
    uniforms.uAccent.value.set(accent);
  }, [accent, uniforms]);

  useFrame((_, dt) => {
    uniforms.uTime.value += dt;
    uniforms.uLevel.value = engine.level;
    window.__signal ??= { frames: 0 };
    window.__signal.frames += 1;

    const mesh = floor.current;
    if (mesh) {
      for (let i = 0; i < BANDS; i++) {
        const n = i / (BANDS - 1);
        const h = 0.05 + engine.bands[i] * 1.15;
        dummy.position.set((n - 0.5) * 7.8, -2.45 + h / 2, -1.2);
        dummy.scale.set(1, h, 1);
        dummy.updateMatrix();
        mesh.setMatrixAt(i, dummy.matrix);
        tint.set(accent).multiplyScalar(0.5 + engine.bands[i] * 0.9);
        mesh.setColorAt(i, tint);
      }
      mesh.instanceMatrix.needsUpdate = true;
      if (mesh.instanceColor) mesh.instanceColor.needsUpdate = true;
    }
  });

  return (
    <>
      {/* backdrop: fullscreen shader quad, always behind */}
      <mesh frustumCulled={false} renderOrder={-1}>
        <planeGeometry args={[2, 2]} />
        <shaderMaterial
          uniforms={uniforms}
          vertexShader={SIGNAL_VERT}
          fragmentShader={SIGNAL_FRAG}
          depthTest={false}
          depthWrite={false}
        />
      </mesh>
      {/* spectrum floor: 31 instanced bars */}
      <instancedMesh ref={floor} args={[undefined, undefined, BANDS]}>
        <boxGeometry args={[0.14, 1, 0.14]} />
        <meshBasicMaterial toneMapped={false} />
      </instancedMesh>
    </>
  );
}

export default function SignalField({
  rootRef,
}: {
  rootRef: React.RefObject<HTMLElement | null>;
}) {
  const visible = useVisible(rootRef);
  const small = typeof matchMedia !== 'undefined' && matchMedia('(max-width: 767px)').matches;
  return (
    <div className="signal-field" aria-hidden="true">
      <Canvas
        dpr={[1, small ? 1.25 : 1.75]}
        frameloop={visible ? 'always' : 'demand'}
        gl={{ antialias: false, alpha: false, powerPreference: 'high-performance' }}
        camera={{ position: [0, 0, 5], fov: 50 }}
      >
        <SignalScene />
      </Canvas>
    </div>
  );
}
