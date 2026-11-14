import { useEffect } from 'react';
import { createSignalEngine, type SignalEngine } from './signal';

let engine: SignalEngine | null = null;
export function getSignalEngine(): SignalEngine {
  return (engine ??= createSignalEngine(31));
}

let driverCount = 0;
let raf = 0;
let last = 0;

function startDriver() {
  last = performance.now();
  const loop = (now: number) => {
    getSignalEngine().tick((now - last) / 1000);
    last = now;
    raf = requestAnimationFrame(loop);
  };
  raf = requestAnimationFrame(loop);
}

function stopDriver() {
  cancelAnimationFrame(raf);
  raf = 0;
}

/**
 * Exactly one rAF advances the signal for every consumer (shader, bars, DOM).
 * Mount once in <App/> under reduced-motion === false.
 */
export function useSignalDriver(enabled: boolean) {
  useEffect(() => {
    if (!enabled) return;
    driverCount += 1;
    if (driverCount === 1) startDriver();
    return () => {
      driverCount -= 1;
      if (driverCount === 0) stopDriver();
    };
  }, [enabled]);
}
