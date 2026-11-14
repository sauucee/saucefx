import { useEffect } from 'react';
import { createSignalEngine, type SignalEngine } from './signal';

let engine: SignalEngine | null = null;
export function getSignalEngine(): SignalEngine {
  return (engine ??= createSignalEngine(31));
}

let driverCount = 0;
let raf = 0;
let last = 0;
let paused = false;

function startDriver() {
  last = performance.now();
  const loop = (now: number) => {
    if (!paused) {
      getSignalEngine().tick((now - last) / 1000);
    }
    last = now;
    raf = requestAnimationFrame(loop);
  };
  raf = requestAnimationFrame(loop);
}

function stopDriver() {
  cancelAnimationFrame(raf);
  raf = 0;
}

function watchMotionPause(): () => void {
  const obs = new MutationObserver(() => {
    paused = document.documentElement.dataset.motion === 'off';
  });
  obs.observe(document.documentElement, { attributeFilter: ['data-motion'] });
  return () => obs.disconnect();
}

let watcherCount = 0;
let unwatch: (() => void) | null = null;

/**
 * Exactly one rAF advances the signal for every consumer (shader, bars, DOM).
 * Mount once in <App/> under reduced-motion === false. Pauses when the user
 * flips the nav MOTION switch (<html data-motion="off">).
 */
export function useSignalDriver(enabled: boolean) {
  useEffect(() => {
    if (!enabled) return;
    driverCount += 1;
    if (driverCount === 1) startDriver();
    if (watcherCount === 0) unwatch = watchMotionPause();
    watcherCount += 1;
    return () => {
      driverCount -= 1;
      watcherCount -= 1;
      if (watcherCount === 0 && unwatch) {
        unwatch();
        unwatch = null;
      }
      if (driverCount === 0) stopDriver();
    };
  }, [enabled]);
}
