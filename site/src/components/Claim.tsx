import { useRef } from 'react';
import gsap from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';
import { useGSAP } from '@gsap/react';
import { getSignalEngine } from '../lib/useSignal';

gsap.registerPlugin(ScrollTrigger, useGSAP);

/**
 * Chapter 2 — the signature moment. Pinned while the visitor scrolls:
 * the audio is flat (engine gain 0), the word admits it, the strike
 * draws itself, "UNTIL NOW." lands and the signal wakes up under your
 * fingers — gain is scrubbed by the same scroll.
 */
export function Claim() {
  const root = useRef<HTMLElement>(null);
  const strike = useRef<SVGLineElement>(null);
  const alive = useRef<HTMLSpanElement>(null);
  const gainOut = useRef<HTMLSpanElement>(null);

  useGSAP(
    () => {
      const mm = gsap.matchMedia();

      mm.add('(prefers-reduced-motion: no-preference)', () => {
        getSignalEngine().gain = 0.06;

        const tl = gsap.timeline({
          defaults: { ease: 'none' },
          scrollTrigger: {
            trigger: root.current,
            start: 'top top',
            end: '+=260%',
            pin: true,
            scrub: 0.6,
            anticipatePin: 1,
            onUpdate(self) {
              // the wake-up: gain rides the second half of the scrub
              const g = gsap.utils.clamp(0.06, 1, (self.progress - 0.45) / 0.45);
              getSignalEngine().gain = g;
              if (gainOut.current)
                gainOut.current.textContent = `${Math.round(g * 100)}%`.padStart(3, '0');
            },
            onLeave() {
              getSignalEngine().gain = 1;
            },
            onEnterBack() {
              getSignalEngine().gain = 0.06;
            },
          },
        });

        tl.fromTo(
          '.claim__word--flat',
          { opacity: 0, y: 60 },
          { opacity: 1, y: 0, duration: 0.8, ease: 'expo.out' },
          0.12,
        );
        // the strike draws across FLAT
        tl.fromTo(
          strike.current,
          { attr: { x2: '12%' } },
          { attr: { x2: '88%' }, duration: 0.5, ease: 'power2.inOut' },
          0.62,
        );
        tl.to('.claim__word--flat', { opacity: 0.28, duration: 0.35 }, 0.95);
        // UNTIL NOW. + the wake-up
        tl.fromTo(
          alive.current,
          { opacity: 0, y: 80 },
          { opacity: 1, y: 0, duration: 0.9, ease: 'expo.out' },
          1.28,
        );
        tl.fromTo('.claim__gain', { opacity: 0 }, { opacity: 1, duration: 0.3 }, 1.5);
      });

      return () => {
        mm.revert();
        getSignalEngine().gain = 1;
      };
    },
    { scope: root },
  );

  return (
    <section ref={root} id="claim" className="claim" aria-label="The claim">
      <div className="claim__pin">
        <div className="claim__stage">
          <h2 className="claim__line claim__word--flat">
            Your audio is flat
            <svg className="claim__strike" viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true">
              <line ref={strike} x1="12%" y1="50%" x2="12%" y2="50%" />
            </svg>
          </h2>
          <span className="claim__line claim__word--alive" ref={alive} aria-hidden="true">
            Until now.
          </span>
        </div>

        <p className="claim__gain label" aria-hidden="true">
          signal <span ref={gainOut}>006%</span>
        </p>
      </div>
    </section>
  );
}
