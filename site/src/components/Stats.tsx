import { useEffect, useRef } from 'react';
import { STATS, VERSION } from '../content/site';
import { prefersReducedMotion } from '../lib/usePrefersReducedMotion';

const pad = (value: number) => String(value).padStart(2, '0');

export function Stats() {
  const section = useRef<HTMLElement>(null);

  useEffect(() => {
    const root = section.current;
    if (!root) return;
    const numbers = Array.from(root.querySelectorAll<HTMLElement>('[data-count]'));

    if (prefersReducedMotion()) {
      numbers.forEach((el) => {
        el.textContent = pad(Number(el.dataset.count ?? 0));
      });
      return;
    }

    let raf = 0;
    let started = false;

    const run = () => {
      const start = performance.now();
      const duration = 1150;

      const frame = (now: number) => {
        const t = Math.min(1, (now - start) / duration);
        // Expo-out: fast settle, no bounce.
        const eased = t === 1 ? 1 : 1 - Math.pow(2, -9 * t);
        numbers.forEach((el) => {
          const target = Number(el.dataset.count ?? 0);
          el.textContent = pad(Math.round(target * eased));
        });
        if (t < 1) raf = requestAnimationFrame(frame);
      };

      raf = requestAnimationFrame(frame);
    };

    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries[0]?.isIntersecting || started) return;
        started = true;
        run();
        observer.disconnect();
      },
      { rootMargin: '-10% 0px -20% 0px', threshold: 0.2 },
    );

    observer.observe(root);
    return () => {
      observer.disconnect();
      if (raf) cancelAnimationFrame(raf);
    };
  }, []);

  return (
    <section className="stats" ref={section} aria-labelledby="spec-title">
      <div className="shell">
        <header className="sec__head">
          <span className="label">01 — Spec</span>
          <h2 id="spec-title" className="sr-only">
            SauceFX in numbers
          </h2>
          <span className="label">SauceFX {VERSION} / Linux</span>
        </header>

        <ul className="stats__grid">
          {STATS.map((stat) => (
            <li className="stat" key={stat.label}>
              <span className="stat__num">
                <span data-count={stat.value}>
                  {prefersReducedMotion() ? pad(stat.value) : '00'}
                </span>
              </span>
              <div className="stat__meta">
                <b>{stat.label}</b>
                <span>{stat.detail}</span>
              </div>
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}
