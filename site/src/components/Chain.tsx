import { useRef } from 'react';
import gsap from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';
import { useGSAP } from '@gsap/react';
import { EFFECTS, EFFECT_NOTES } from '../content/site';

gsap.registerPlugin(ScrollTrigger, useGSAP);

/**
 * Chapter 4 — the chain. A pinned horizontal rack: five modules wired by a
 * patch cable that the scroll draws, pulse to pulse, in signal order.
 */
const PLOTS: Record<string, string> = {
  Clarity: 'M0,78 C22,74 34,30 54,26 C74,22 84,58 104,54 S122,50 128,48',
  Ambience: 'M0,72 C30,70 44,44 62,40 C84,36 96,48 112,44 S124,40 128,38',
  'Surround Sound': 'M0,76 C20,72 30,36 52,34 C78,32 88,64 108,60 S122,56 128,54',
  'Dynamic Boost': 'M0,84 C26,80 40,26 60,22 C82,18 94,52 112,46 S124,40 128,36',
  'Bass Boost': 'M0,20 C24,18 40,22 60,40 C80,58 100,74 118,78 S124,80 128,80',
};

export function Chain() {
  const root = useRef<HTMLElement>(null);
  const track = useRef<HTMLDivElement>(null);
  const cable = useRef<SVGPathElement>(null);

  useGSAP(
    () => {
      const mm = gsap.matchMedia();
      mm.add('(prefers-reduced-motion: no-preference)', () => {
        const draw = gsap.fromTo(
          cable.current,
          { strokeDashoffset: 1 },
          { strokeDashoffset: 0, ease: 'none' },
        );
        gsap.to(track.current, {
          x: () => -(track.current!.scrollWidth - window.innerWidth),
          ease: 'none',
          scrollTrigger: {
            trigger: root.current,
            start: 'top top',
            end: () => `+=${track.current!.scrollWidth - window.innerWidth}`,
            pin: true,
            scrub: 0.6,
            anticipatePin: 1,
            invalidateOnRefresh: true,
            onUpdate(self) {
              const active = Math.min(
                EFFECTS.length - 1,
                Math.floor(self.progress * EFFECTS.length),
              );
              root.current
                ?.querySelectorAll('.chain__module')
                .forEach((el, i) => el.classList.toggle('is-hot', i === active));
            },
          },
        });
        ScrollTrigger.create({
          trigger: root.current,
          start: 'top top',
          end: () => `+=${track.current!.scrollWidth - window.innerWidth}`,
          animation: draw,
          scrub: 0.6,
        });
      });
      return () => mm.revert();
    },
    { scope: root },
  );

  const total = String(EFFECTS.length).padStart(2, '0');

  return (
    <section ref={root} id="chain" className="chain" aria-label="The effect chain">
      <div className="chain__pin">
        <header className="shell chain__head">
          <span className="label">Chapter 02 — the chain</span>
          <span className="label">Five effects, wired in signal order</span>
        </header>

        <div className="chain__viewport">
          {/* the patch cable — drawn by scroll, behind the modules */}
          <svg className="chain__cable" viewBox="0 0 2000 100" preserveAspectRatio="none" aria-hidden="true">
            <path
              ref={cable}
              d="M0,50 C120,18 240,82 360,50 S600,18 720,50 S960,82 1080,50 S1320,18 1440,50 S1680,82 1800,50 L2000,50"
              pathLength={1}
              vectorEffect="non-scaling-stroke"
            />
          </svg>

          <div className="chain__track" ref={track}>
            {EFFECTS.map((effect, i) => (
              <article key={effect} className={`chain__module${i === 0 ? ' is-hot' : ''}`}>
                <span className="label chain__idx">
                  {String(i + 1).padStart(2, '0')} / {total}
                </span>
                <svg className="chain__plot" viewBox="0 0 128 96" aria-hidden="true">
                  <path d={PLOTS[effect]} />
                </svg>
                <h3 className="chain__name">{effect}</h3>
                <p className="chain__note">{EFFECT_NOTES[effect]}</p>
              </article>
            ))}
          </div>
        </div>

        <footer className="shell chain__foot">
          <span className="label">Drag · scroll — the signal passes through each module</span>
        </footer>
      </div>
    </section>
  );
}
