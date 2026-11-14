import { useEffect, useMemo, useRef, useState } from 'react';
import * as THREE from 'three';
import { Canvas, useFrame, useThree } from '@react-three/fiber';
import { EffectComposer } from 'three/examples/jsm/postprocessing/EffectComposer.js';
import { RenderPass } from 'three/examples/jsm/postprocessing/RenderPass.js';
import { UnrealBloomPass } from 'three/examples/jsm/postprocessing/UnrealBloomPass.js';
import { getSignalEngine } from '../lib/useSignal';
import { useAccentColor, useTokenColor } from '../lib/useTokenColor';
import { motionOff } from '../lib/motionPreference';
import { SIGNAL_FRAG, SIGNAL_VERT } from './shaders/signal';

/** Tracks <html data-motion="off"> so the whole canvas can freeze. */
function useMotionOff(): boolean {
  const [off, setOff] = useState(motionOff);
  useEffect(() => {
    const obs = new MutationObserver(() => setOff(motionOff()));
    obs.observe(document.documentElement, { attributeFilter: ['data-motion'] });
    return () => obs.disconnect();
  }, []);
  return off;
}

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

/* ---------------------------------------------------------------- bloom */

function Bloom({ strength }: { strength: number }) {
  const gl = useThree((s) => s.gl);
  const scene = useThree((s) => s.scene);
  const camera = useThree((s) => s.camera);
  const size = useThree((s) => s.size);

  const composer = useMemo(() => {
    const c = new EffectComposer(gl);
    c.addPass(new RenderPass(scene, camera));
    c.addPass(
      new UnrealBloomPass(
        new THREE.Vector2(size.width, size.height),
        strength,
        0.9, // radius
        0.32, // threshold — the signal emits
      ),
    );
    return c;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [gl, scene, camera, strength]);

  useEffect(() => {
    composer.setSize(size.width, size.height);
    composer.setPixelRatio(Math.min(window.devicePixelRatio, 1.75));
  }, [composer, size]);

  useEffect(() => () => composer.dispose(), [composer]);

  useFrame((_, dt) => composer.render(dt), 1);
  return null;
}

/* -------------------------------------------------------- waveform lines */

const LINES = 30;
const SEGS = 144;
const WIDTH = 13.2;

/**
 * The hero IS the signal: horizontal DAW-style lines whose vertical
 * displacement is driven by the live 31-band engine — a glowing waveform
 * field that breathes with the demo (or the user's mic).
 */
function WaveformLines({ isLight }: { isLight: boolean }) {
  const engine = getSignalEngine();
  const accent = useAccentColor();
  const mat = useRef<THREE.LineBasicMaterial>(null);
  const geom = useRef<THREE.BufferGeometry>(null);
  const positions = useMemo(
    () => new Float32Array(LINES * SEGS * 2 * 3),
    [],
  );
  const t = useRef(0);

  useEffect(() => {
    if (mat.current) mat.current.color.set(accent);
  }, [accent]);

  useFrame((_, dt) => {
    t.current += dt;
    const time = t.current;
    const arr = positions;
    let k = 0;
    for (let l = 0; l < LINES; l++) {
      const bn = l / (LINES - 1); // 0 lows … 1 highs
      const band = engine.bands[Math.min(30, (bn * 30) | 0)];
      const baseY = 2.5 - bn * 4.35; // fills +2.5 … −1.85
      const amp = 0.04 + band * 0.55;
      const freq = 1.15 + bn * 2.3;
      const speed = 0.9 + bn * 1.9;
      const phase = l * 0.83;
      let prevY = 0;
      for (let s = 0; s <= SEGS; s++) {
        const x = -WIDTH / 2 + (s / SEGS) * WIDTH;
        const centred = 0.3 + 0.7 * Math.pow(1 - Math.abs(x) / (WIDTH / 2), 1.4);
        const y =
          baseY +
          Math.sin(x * freq + time * speed + phase) * amp * centred +
          Math.sin(x * 5.3 - time * (speed * 0.6)) * amp * 0.24 * centred;
        if (s > 0) {
          arr[k++] = x - WIDTH / SEGS;
          arr[k++] = prevY;
          arr[k++] = 0;
          arr[k++] = x;
          arr[k++] = y;
          arr[k++] = 0;
        }
        prevY = y;
      }
    }
    const attr = geom.current?.attributes.position as THREE.BufferAttribute | undefined;
    if (attr) attr.needsUpdate = true;
  });

  return (
    <lineSegments frustumCulled={false} renderOrder={0}>
      <bufferGeometry ref={geom}>
        <bufferAttribute attach="attributes-position" args={[positions, 3]} />
      </bufferGeometry>
      <lineBasicMaterial
        ref={mat}
        color="#ff2e57"
        transparent
        opacity={isLight ? 0.62 : 0.5}
        // Additive vanishes on light themes (adds toward white) — draw ink on paper instead.
        blending={isLight ? THREE.NormalBlending : THREE.AdditiveBlending}
        depthWrite={false}
        toneMapped={false}
      />
    </lineSegments>
  );
}

/* ---------------------------------------------------------------- scene */

const BANDS = 31;

function SignalScene({ metal, isLight }: { metal: string; isLight: boolean }) {
  const engine = getSignalEngine();
  const accent = useAccentColor();
  const floor = useRef<THREE.InstancedMesh>(null);
  const floorMat = useRef<THREE.MeshStandardMaterial>(null);
  const dummy = useMemo(() => new THREE.Object3D(), []);
  const tint = useMemo(() => new THREE.Color(), []);
  const uniforms = useMemo(
    () => ({
      uTime: { value: 0 },
      uLevel: { value: 0 },
      uAccent: { value: new THREE.Color('#ff2e57') },
      uMetal: { value: new THREE.Color('#050506') },
      uMouse: { value: new THREE.Vector2(0.5, 0.5) },
    }),
    [],
  );

  useEffect(() => {
    uniforms.uAccent.value.set(accent);
  }, [accent, uniforms]);

  useEffect(() => {
    uniforms.uMetal.value.set(metal);
  }, [metal, uniforms]);

  useEffect(() => {
    if (floorMat.current) floorMat.current.emissive.set(accent);
  }, [accent]);

  useFrame((_, dt) => {
    uniforms.uTime.value += dt;
    uniforms.uLevel.value = engine.level;
    const m = window.__signalMouse;
    if (m) uniforms.uMouse.value.set(m.x, m.y);
    window.__signal ??= { frames: 0 };
    window.__signal.frames += 1;

    const mesh = floor.current;
    if (mesh) {
      for (let i = 0; i < BANDS; i++) {
        const n = i / (BANDS - 1);
        const h = 0.05 + engine.bands[i] * 1.35;
        dummy.position.set((n - 0.5) * 9.6, -2.55 + h / 2, -1.2);
        dummy.scale.set(1, h, 1);
        dummy.updateMatrix();
        mesh.setMatrixAt(i, dummy.matrix);
        tint.set(accent);
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

      {/* the signal: glowing waveform lines across the whole hero */}
      <WaveformLines isLight={isLight} />

      {/* spectrum floor: emissive bars catching the bloom */}
      <instancedMesh ref={floor} args={[undefined, undefined, BANDS]} frustumCulled={false}>
        <boxGeometry args={[0.12, 1, 0.12]} />
        <meshStandardMaterial
          ref={floorMat}
          color="#000000"
          emissive="#ff2e57"
          emissiveIntensity={isLight ? 0.85 : 1.5}
          toneMapped={false}
        />
      </instancedMesh>
    </>
  );
}

/* -------------------------------------------------------------- wrapper */

function isLightColor(hex: string): boolean {
  const c = new THREE.Color(hex);
  return (c.r + c.g + c.b) / 3 > 0.5;
}

export default function SignalField({
  rootRef,
}: {
  rootRef: React.RefObject<HTMLElement | null>;
}) {
  const visible = useVisible(rootRef);
  const motionPaused = useMotionOff();
  const metal = useTokenColor('--bg-deep', '#050506');
  const small = typeof matchMedia !== 'undefined' && matchMedia('(max-width: 767px)').matches;
  const isLight = isLightColor(metal);

  useEffect(() => {
    const onMove = (event: PointerEvent) => {
      window.__signalMouse ??= { x: 0.5, y: 0.5 };
      window.__signalMouse.x = event.clientX / window.innerWidth;
      window.__signalMouse.y = 1 - event.clientY / window.innerHeight;
    };
    window.addEventListener('pointermove', onMove, { passive: true });
    return () => window.removeEventListener('pointermove', onMove);
  }, []);

  return (
    <div className="signal-field" aria-hidden="true">
      <Canvas
        dpr={[1, small ? 1.25 : 1.6]}
        frameloop={visible && !motionPaused ? 'always' : 'demand'}
        gl={{ antialias: false, alpha: false, powerPreference: 'high-performance' }}
        camera={{ position: [0, 0, 5], fov: 50 }}
      >
        <SignalScene metal={metal} isLight={isLight} />
        {/* bloom only on dark: on cream it would bloom the paper itself */}
        {!isLight && <Bloom strength={1.05} />}
      </Canvas>
    </div>
  );
}
