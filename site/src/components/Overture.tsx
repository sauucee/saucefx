import { useEffect, useRef } from 'react';
import gsap from 'gsap';
import { Reveal } from './Reveal';
import { getSignalEngine } from '../lib/useSignal';
import { Readouts } from './Readouts';
import { VERSION } from '../content/site';

/**
 * Chapter 1 — the void. A whisper of signal, a title, a promise.
 * The engine idles near-flat here; The Claim wakes it up.
 */
export function Overture() {
  const root = useRef<HTMLElement>(null);

  useEffect(() => {
    getSignalEngine().gain = 0.12;
    // the word leans with the pointer — quiet life in the void
    if (window.matchMedia('(pointer: fine)').matches && !window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      const word = root.current?.querySelector<HTMLElement>('#hero-title');
      if (word) {
        const xTo = gsap.quickTo(word, 'x', { duration: 1.1, ease: 'power3.out' });
        const yTo = gsap.quickTo(word, 'y', { duration: 1.1, ease: 'power3.out' });
        const onMove = (ev: PointerEvent) => {
          const nx = (ev.clientX / window.innerWidth) * 2 - 1;
          const ny = (ev.clientY / window.innerHeight) * 2 - 1;
          xTo(nx * -14);
          yTo(ny * -8);
        };
        const onLeave = () => {
          xTo(0);
          yTo(0);
        };
        window.addEventListener('pointermove', onMove, { passive: true });
        window.addEventListener('pointerleave', onLeave);
        return () => {
          window.removeEventListener('pointermove', onMove);
          window.removeEventListener('pointerleave', onLeave);
          getSignalEngine().gain = 1;
        };
      }
    }
    return () => {
      getSignalEngine().gain = 1;
    };
  }, []);

  return (
    <section ref={root} id="overture" className="overture" aria-label="Introduction">
      <div className="overture__mid">
        <Reveal as="h1" id="hero-title" className="overture__word" mode="chars" immediate>
          SauceFX
        </Reveal>
        <span className="overture__sub">v{VERSION} · the FxSound DSP engine, native</span>
      </div>

      <div className="overture__foot">
        <Readouts />
        <span className="overture__cue" aria-hidden="true" />
      </div>
    </section>
  );
}
