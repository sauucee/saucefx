import { useEffect, type RefObject } from 'react';
import { getLenis } from './useSmoothScroll';
import { prefersReducedMotion } from './usePrefersReducedMotion';

/**
 * Writes Lenis scroll velocity into --skew on the element. Under reduced
 * motion Lenis never mounts, so this idles at 0deg.
 */
export function useLenisVelocity(ref: RefObject<HTMLElement | null>) {
  useEffect(() => {
    if (prefersReducedMotion()) return;
    let raf = 0;
    const loop = () => {
      const v = getLenis()?.velocity ?? 0;
      const el = ref.current;
      if (el) {
        el.style.setProperty('--skew', `${Math.max(-6, Math.min(6, -v * 0.6))}deg`);
      }
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  }, [ref]);
}
