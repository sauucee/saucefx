import { useRef } from 'react';
import gsap from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';
import { useGSAP } from '@gsap/react';
import { EFFECTS, EFFECT_NOTES } from '../content/site';

gsap.registerPlugin(ScrollTrigger, useGSAP);

/**
 * One hand-authored response plot per effect (128×96 box). These literals keep
 * the five signature shapes stable for the film look: presence lift, broad
 * room, mid scoop, upward tilt, low shelf.
 */
const PLOTS: Record<string, string> = {
  Clarity: 'M0,78 C22,74 34,30 54,26 C74,22 84,58 104,54 S122,50 128,48',
  Ambience: 'M0,72 C30,70 44,44 62,40 C84,36 96,48 112,44 S124,40 128,38',
  'Surround Sound': 'M0,76 C20,72 30,36 52,34 C78,32 88,64 108,60 S122,56 128,54',
  'Dynamic Boost': 'M0,84 C26,80 40,26 60,22 C82,18 94,52 112,46 S124,40 128,36',
  'Bass Boost': 'M0,20 C24,18 40,22 60,40 C80,58 100,74 118,78 S124,80 128,80',
};

const TOTAL = String(EFFECTS.length).padStart(2, '0');

/**
 * The rack as a film: one pinned viewport, five frames, scroll is the
 * transport. The counter and scrub rail are updated inside onUpdate so a
 * scrub never triggers a React render.
 */
export function Frames() {
  const root = useRef<HTMLElement>(null);
  const counter = useRef<HTMLSpanElement>(null);
  const rail = useRef<HTMLSpanElement>(null);

  useGSAP(
    () => {
      const slides = gsap.utils.toArray<HTMLElement>('.frames__slide');
      const mm = gsap.matchMedia();

      mm.add('(prefers-reduced-motion: no-preference)', () => {
        const tl = gsap.timeline({
          defaults: { ease: 'none' },
          scrollTrigger: {
            trigger: root.current,
            start: 'top top',
            end: '+=400%',
            pin: true,
            scrub: 1,
            anticipatePin: 1,
            onUpdate(self) {
              const idx = Math.min(
                EFFECTS.length - 1,
                Math.floor(self.progress * EFFECTS.length),
              );
              if (counter.current) counter.current.textContent = String(idx + 1).padStart(2, '0');
              if (rail.current) rail.current.style.setProperty('--scrub', self.progress.toFixed(4));
              slides.forEach((slide, s) => slide.classList.toggle('is-active', s === idx));
            },
          },
        });

        slides.forEach((slide, i) => {
          if (i > 0) {
            tl.fromTo(slide, { autoAlpha: 0, y: 40 }, { autoAlpha: 1, y: 0, duration: 1 }).to(
              slides[i - 1],
              { autoAlpha: 0, y: -40, duration: 0.6 },
              '<',
            );
          }
          const path = slide.querySelector<SVGPathElement>('.frames__plot path');
          if (path) {
            const len = path.getTotalLength();
            gsap.set(path, { strokeDasharray: len, strokeDashoffset: len });
            tl.to(path, { strokeDashoffset: 0, duration: 0.8 }, i * 1 + 0.25);
          }
        });
      });

      return () => mm.revert();
    },
    { scope: root },
  );

  return (
    <section
      ref={root}
      id="frames"
      className="frames"
      aria-label="The effect rack, frame by frame"
    >
      <div className="frames__pin">
        <header className="frames__head">
          <span className="label" aria-hidden="true">
            Frame <span ref={counter}>01</span>/{TOTAL}
          </span>
          <span ref={rail} className="frames__scrub" aria-hidden="true" />
        </header>

        <div className="frames__stage">
          {EFFECTS.map((effect, i) => (
            <article key={effect} className="frames__slide">
              <svg className="frames__plot" viewBox="0 0 128 96" aria-hidden="true">
                <path d={PLOTS[effect]} />
              </svg>
              <h2 className="frames__title">{effect}</h2>
              <p className="frames__note">{EFFECT_NOTES[effect]}</p>
              <span className="label frames__index" aria-hidden="true">
                {String(i + 1).padStart(2, '0')} / {TOTAL}
              </span>
            </article>
          ))}
        </div>
      </div>
    </section>
  );
}
