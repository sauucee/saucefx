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

function useVisible() {
  // Fixed backdrop — alive whenever the document is visible.
  const [visible, setVisible] = useState(!document.hidden);
  useEffect(() => {
    const onVis = () => setVisible(!document.hidden);
    document.addEventListener('visibilitychange', onVis);
    return () => document.removeEventListener('visibilitychange', onVis);
  }, []);
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
        1.0, // radius — soft, wide falloff
        0.28, // threshold — only the signal emits
      ),
    );
    return c;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [gl, scene, camera, strength]);

  useEffect(() => {
    composer.setSize(size.width, size.height);
    composer.setPixelRatio(Math.min(window.devicePixelRatio, 1.6));
  }, [composer, size]);

  useEffect(() => () => composer.dispose(), [composer]);

  useFrame((_, dt) => composer.render(dt), 1);
  return null;
}

/* ------------------------------------------------- waveform terrain */

const ROWS = 22; // 1 hero row + rows receding into fog
const SEGS = 150;
const WIDTH = 15;

/**
 * The Unknown Pleasures shot: rows of the live waveform receding into fog.
 * Every row is the same signal at an earlier moment; depth gives it a body.
 */
function SignalTerrain({ isLight }: { isLight: boolean }) {
  const engine = getSignalEngine();
  const accent = useAccentColor();
  const mat = useRef<THREE.LineBasicMaterial>(null);
  const geom = useRef<THREE.BufferGeometry>(null);
  const rig = useRef<THREE.Group>(null);
  const positions = useMemo(() => new Float32Array(ROWS * SEGS * 2 * 3), []);
  const colors = useMemo(() => new Float32Array(ROWS * SEGS * 2 * 3), []);
  const accentC = useMemo(() => new THREE.Color('#ff2e57'), []);
  const heatA = useMemo(() => new THREE.Color('#0ea5b7'), []);
  const heatB = useMemo(() => new THREE.Color('#7c5cff'), []);
  const heatC = useMemo(() => new THREE.Color('#ff2e57'), []);
  const heat = useMemo(() => new THREE.Color(), []);
  const t = useRef(0);
  const mouse = useRef({ x: 0, y: 0 });

  useEffect(() => {
    accentC.set(accent);
    heatC.set(accent);
  }, [accent, accentC, heatC]);

  useEffect(() => {
    if (mat.current) mat.current.opacity = isLight ? 0.66 : 0.95;
  }, [isLight]);

  useFrame((_, dt) => {
    t.current += dt;
    const time = t.current;
    const level = engine.level;
    const m = window.__signalMouse;
    if (m) {
      mouse.current.x += (m.x - 0.5 - mouse.current.x) * 0.04;
      mouse.current.y += (m.y - 0.5 - mouse.current.y) * 0.04;
    }
    if (rig.current) {
      rig.current.rotation.y = mouse.current.x * 0.16 + Math.sin(time * 0.07) * 0.012;
      rig.current.rotation.x = -mouse.current.y * 0.05;
    }

    const arr = positions;
    let k = 0;
    for (let l = 0; l < ROWS; l++) {
      const depth = l / (ROWS - 1); // 0 near, 1 far
      const z = -depth * 15; // recede into fog
      const baseY = 0.6 + depth * 3.5; // rows wrap behind the word, rising to the horizon
      // energy: near rows shout, far rows whisper (fog)
      const fog = Math.pow(1 - depth, 1.7);
      const band = engine.bands[Math.min(30, ((1 - depth) * 30) | 0)];
      // the landscape exists even when the signal sleeps — base undulation
      const amp = (0.15 + level * 0.12 + band * 0.55) * (0.4 + fog * 0.6);
      const trailTime = time - depth * 0.5; // far rows are the past
      let prevY = 0;
      for (let s = 0; s <= SEGS; s++) {
        const x = -WIDTH / 2 + (s / SEGS) * WIDTH;
        const centred = 0.25 + 0.75 * Math.pow(1 - Math.abs(x) / (WIDTH / 2), 1.5);
        const y =
          baseY +
          (Math.sin(x * 1.05 + trailTime * 0.5 + l * 0.35) * amp +
            Math.sin(x * 2.2 - trailTime * 0.32 + 1.6) * amp * 0.42 +
            Math.exp(-((x - Math.sin(trailTime * 0.13) * 2.8) ** 2) / 2.2) * level * 0.34) *
            centred;
        if (s > 0) {
          arr[k++] = x - WIDTH / SEGS;
          arr[k++] = prevY;
          arr[k++] = z;
          arr[k++] = x;
          arr[k++] = y;
          arr[k++] = z;
        }
        prevY = y;
      }
    }
    const attr = geom.current?.attributes.position as THREE.BufferAttribute | undefined;
    if (attr) attr.needsUpdate = true;

    // heat gradient along x, faded by fog — baked per row
    const cArr = colors;
    let c = 0;
    for (let l = 0; l < ROWS; l++) {
      const depth = l / (ROWS - 1);
      const fog = Math.pow(1 - depth, 1.7);
      const intensity = (0.06 + fog * 0.94) * 0.9;
      for (let s = 0; s <= SEGS; s++) {
        const u = s / SEGS;
        if (u < 0.5) heat.copy(heatA).lerp(heatB, u * 2);
        else heat.copy(heatB).lerp(accentC, (u - 0.5) * 2);
        cArr[c++] = heat.r * intensity;
        cArr[c++] = heat.g * intensity;
        cArr[c++] = heat.b * intensity;
      }
    }
    const cAttr = geom.current?.attributes.color as THREE.BufferAttribute | undefined;
    if (cAttr) cAttr.needsUpdate = true;
  });

  return (
    <group ref={rig}>
      <lineSegments frustumCulled={false} renderOrder={0}>
        <bufferGeometry ref={geom}>
          <bufferAttribute attach="attributes-position" args={[positions, 3]} />
          <bufferAttribute attach="attributes-color" args={[colors, 3]} />
        </bufferGeometry>
        <lineBasicMaterial
          ref={mat}
          vertexColors
          transparent
          blending={isLight ? THREE.NormalBlending : THREE.AdditiveBlending}
          depthWrite={false}
          toneMapped={false}
          fog={false}
        />
      </lineSegments>
    </group>
  );
}

/* ------------------------------------------------------------ dust */

const DUST = 240;

/** Slow-drifting atmosphere. Barely visible; adds depth to the void. */
function Dust() {
  const pts = useRef<THREE.Points>(null);
  const positions = useMemo(() => {
    const a = new Float32Array(DUST * 3);
    for (let i = 0; i < DUST; i++) {
      a[i * 3] = (Math.random() - 0.5) * 12;
      a[i * 3 + 1] = (Math.random() - 0.5) * 6.4;
      a[i * 3 + 2] = -0.4 - Math.random() * 2.2;
    }
    return a;
  }, []);

  useFrame((_, dt) => {
    const attr = pts.current?.geometry.attributes.position as THREE.BufferAttribute | undefined;
    if (!attr) return;
    const a = attr.array as Float32Array;
    for (let i = 0; i < DUST; i++) {
      a[i * 3 + 1] += dt * (0.02 + (i % 5) * 0.004);
      if (a[i * 3 + 1] > 3.4) a[i * 3 + 1] = -3.4;
    }
    attr.needsUpdate = true;
  });

  return (
    <points ref={pts} frustumCulled={false}>
      <bufferGeometry>
        <bufferAttribute attach="attributes-position" args={[positions, 3]} />
      </bufferGeometry>
      <pointsMaterial
        size={0.018}
        color="#8a8a92"
        transparent
        opacity={0.22}
        blending={THREE.AdditiveBlending}
        depthWrite={false}
        sizeAttenuation
        toneMapped={false}
      />
    </points>
  );
}

/* ---------------------------------------------------------------- scene */

function SignalScene({ metal, isLight }: { metal: string; isLight: boolean }) {
  const engine = getSignalEngine();
  const accent = useAccentColor();
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
    // freed palette: dark theme sits in deep blue-black, not pure black
    if (!isLightColor(metal)) {
      uniforms.uMetal.value.lerp(new THREE.Color('#0a1020'), 0.55);
    }
  }, [metal, uniforms]);

  useFrame((_, dt) => {
    uniforms.uTime.value += dt;
    uniforms.uLevel.value = engine.level;
    const m = window.__signalMouse;
    if (m) uniforms.uMouse.value.set(m.x, m.y);
    window.__signal ??= { frames: 0 };
    window.__signal.frames += 1;
  });

  return (
    <>
      {/* backdrop: near-black void with atmosphere */}
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

      <Dust />
      <SignalTerrain isLight={isLight} />
    </>
  );
}

/* -------------------------------------------------------------- wrapper */

function isLightColor(hex: string): boolean {
  const c = new THREE.Color(hex);
  return (c.r + c.g + c.b) / 3 > 0.5;
}

export default function SignalField() {
  const visible = useVisible();
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
        {!isLight && <Bloom strength={0.72} />}
      </Canvas>
    </div>
  );
}
