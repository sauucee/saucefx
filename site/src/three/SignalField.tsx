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

/* ------------------------------------------------- persistence waveform */

const TRAILS = 6; // 1 primary + 5 persistence echoes
const SEGS = 200;
const WIDTH = 12.6;

/**
 * ONE elegant oscilloscope line, displaced by the live 31-band signal, with
 * persistence-trail echoes of its own past — the classic scope look. All
 * trails share the same shape so the field reads as a single object.
 */
function SignalLine({ isLight }: { isLight: boolean }) {
  const engine = getSignalEngine();
  const accent = useAccentColor();
  const mat = useRef<THREE.LineBasicMaterial>(null);
  const geom = useRef<THREE.BufferGeometry>(null);
  const positions = useMemo(() => new Float32Array(TRAILS * SEGS * 2 * 3), []);
  const colors = useMemo(() => new Float32Array(TRAILS * SEGS * 2 * 3), []);
  const accentC = useMemo(() => new THREE.Color('#ff2e57'), []);
  const heatA = useMemo(() => new THREE.Color('#0ea5b7'), []); // deep spectrogram teal
  const heatB = useMemo(() => new THREE.Color('#7c5cff'), []); // violet mid
  const heatC = useMemo(() => new THREE.Color('#ff2e57'), []); // accent end
  const heat = useMemo(() => new THREE.Color(), []);
  const t = useRef(0);

  useEffect(() => {
    accentC.set(accent);
    heatC.set(accent);
  }, [accent, accentC, heatC]);

  // spectrogram heat along x (teal → violet → accent), trail intensity baked in
  useEffect(() => {
    for (let l = 0; l < TRAILS; l++) {
      const intensity = l === 0 ? 1 : 0.3 * Math.pow(0.66, l - 1);
      for (let s = 0; s <= SEGS; s++) {
        const u = s / SEGS;
        if (u < 0.5) heat.copy(heatA).lerp(heatB, u * 2);
        else heat.copy(heatB).lerp(heatC, (u - 0.5) * 2);
        const vi = (l * (SEGS + 1) + s) * 3;
        colors[vi] = heat.r * intensity;
        colors[vi + 1] = heat.g * intensity;
        colors[vi + 2] = heat.b * intensity;
      }
    }
    const cAttr = geom.current?.attributes.color as THREE.BufferAttribute | undefined;
    if (cAttr) cAttr.needsUpdate = true;
  }, [accentC, heatC, heatA, heatB, heat, colors, geom]);

  useEffect(() => {
    if (mat.current) mat.current.opacity = isLight ? 0.62 : 0.9;
  }, [isLight]);

  // waveform(x, time): slow swell + second harmonic + a traveling energy packet
  const sample = (x: number, time: number, level: number) => {
    const a1 = 0.14 + level * 0.1;
    const a2 = 0.06;
    const packet = Math.exp(-((x - Math.sin(time * 0.13) * 3.1) ** 2) / 2.6) * level * 0.3;
    return (
      Math.sin(x * 1.15 + time * 0.5) * a1 +
      Math.sin(x * 2.3 - time * 0.34 + 1.7) * a2 +
      packet
    );
  };

  useFrame((_, dt) => {
    t.current += dt;
    const arr = positions;
    let k = 0;
    for (let l = 0; l < TRAILS; l++) {
      const time = t.current - l * 0.24; // each echo is the line's own past
      const baseY = -0.3 - l * 0.13; // crosses behind the window's lower third
      const level = engine.level;
      let prevY = 0;
      for (let s = 0; s <= SEGS; s++) {
        const x = -WIDTH / 2 + (s / SEGS) * WIDTH;
        const y = baseY + sample(x, time, level);
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
        <bufferAttribute attach="attributes-color" args={[colors, 3]} />
      </bufferGeometry>
      <lineBasicMaterial
        ref={mat}
        vertexColors
        transparent
        blending={isLight ? THREE.NormalBlending : THREE.AdditiveBlending}
        depthWrite={false}
        toneMapped={false}
      />
    </lineSegments>
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
      <SignalLine isLight={isLight} />
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
