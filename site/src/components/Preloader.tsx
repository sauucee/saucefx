import { useRef } from 'react';
import { useGSAP } from '@gsap/react';
import { gsap } from 'gsap';
import { prefersReducedMotion } from '../lib/usePrefersReducedMotion';
import logo from '../assets/saucefx-logo.svg';
import { EFFECTS, VERSION } from '../content/site';

const SESSION_KEY = 'saucefx-site:loaded';
const BAR_COUNT = 31;

/** The curtain plays once per session, never for reduced-motion visitors. */
export function shouldPlayPreloader(): boolean {
  if (prefersReducedMotion()) return false;
  try {
    return sessionStorage.getItem(SESSION_KEY) !== '1';
  } catch {
    return true;
  }
}

/**
 * `onReveal` fires as the curtain starts to lift so the hero enters through the
 * moving edge; `onDone` fires after the curtain has left and can be unmounted.
 */
export function Preloader({
  onReveal,
  onDone,
}: {
  onReveal: () => void;
  onDone: () => void;
}) {
  const root = useRef<HTMLDivElement>(null);
  const counter = useRef<HTMLSpanElement>(null);

  useGSAP(
    () => {
      const markSeen = () => {
        try {
          sessionStorage.setItem(SESSION_KEY, '1');
        } catch {
          /* ignore */
        }
      };

      const bars = gsap.utils.toArray<HTMLElement>('.pre__bar');
      const text = gsap.utils.toArray<HTMLElement>('.pre__fade');
      const number = { value: 0 };

      document.documentElement.style.overflow = 'hidden';

      const ticker = gsap.ticker;
      ticker.lagSmoothing(0);

      const tl = gsap.timeline({
        defaults: { ease: 'expo.out' },
        onComplete: () => {
          document.documentElement.style.overflow = '';
          onDone();
        },
      });

      // Bars grow into a rough spectrum, then breathe while the count runs.
      tl.to(
        bars,
        {
          scaleY: (i: number) => 0.14 + Math.abs(Math.sin(i * 0.73 + 0.5)) * 0.86,
          duration: 0.7,
          stagger: 0.014,
          ease: 'power3.out',
        },
        0,
      );

      const breathe = gsap.to(bars, {
        scaleY: () => gsap.utils.random(0.12, 1),
        duration: 0.42,
        ease: 'sine.inOut',
        stagger: { each: 0.022, from: 'center' },
        repeat: -1,
        yoyo: true,
        delay: 0.6,
      });

      tl.to(
        number,
        {
          value: 100,
          duration: 1.28,
          ease: 'power2.inOut',
          onUpdate: () => {
            if (counter.current) {
              counter.current.textContent = String(Math.round(number.value)).padStart(2, '0');
            }
          },
        },
        0.06,
      );

      tl.fromTo(text, { y: 16, opacity: 0 }, { y: 0, opacity: 1, duration: 0.8, stagger: 0.06 }, 0);

      // Flatten the meters into a single line, then lift the curtain.
      breathe.kill();
      tl.to(bars, { scaleY: 0.015, duration: 0.3, stagger: 0.006, ease: 'power2.in' }, 1.46);
      tl.to(text, { opacity: 0, y: -10, duration: 0.3, stagger: 0.02, ease: 'power2.in' }, 1.42);
      tl.call(
        () => {
          markSeen();
          onReveal();
        },
        undefined,
        1.5,
      );
      tl.to(
        '.pre__panel',
        { yPercent: -101, duration: 0.95, ease: 'expo.inOut', stagger: 0.045 },
        1.52,
      );
      tl.to(root.current, { autoAlpha: 0, duration: 0.2, ease: 'none' }, 2.3);

      return () => {
        document.documentElement.style.overflow = '';
        breathe.kill();
        markSeen();
        ticker.lagSmoothing(500);
      };
    },
    { scope: root },
  );

  return (
    <div className="pre" ref={root} aria-hidden="true">
      <div className="pre__panels">
        {Array.from({ length: 6 }, (_, i) => (
          <span className="pre__panel" key={i} />
        ))}
      </div>

      <div className="pre__top pre__fade">
        <span className="pre__brand">
          <img src={logo} alt="" width={22} height={22} />
          SauceFX
        </span>
        <span className="label">Sound EQ for Linux</span>
        <span className="label">v{VERSION}</span>
      </div>

      <div className="pre__mid">
        <img
          className="pre__mark"
          src={logo}
          alt=""
          aria-hidden="true"
          width={420}
          height={420}
        />
        <div className="pre__count pre__fade">
          <span ref={counter}>00</span>
          <b>%</b>
        </div>

        <div className="pre__title pre__fade">
          <span>Tuning the rack</span>
          <span className="label">
            <span className="dot" /> DfxDsp · PipeWire
          </span>
        </div>
      </div>

      <div className="pre__bars">
        {Array.from({ length: BAR_COUNT }, (_, i) => (
          <span key={i} className="pre__bar" />
        ))}
      </div>

      <div className="pre__bottom pre__fade">
        <span className="label">{EFFECTS.join(' · ')}</span>
        <span className="label label--ink">Loading signal</span>
      </div>
    </div>
  );
}
