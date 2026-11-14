import { useEffect, useRef } from 'react';
import { STATS } from '../content/site';
import { prefersReducedMotion } from '../lib/usePrefersReducedMotion';

const pad = (value: number) => String(value).padStart(2, '0');

/**
 * Chapter 5 — the proof. Three enormous numbers, editorial layout,
 * asymmetric — no cards, no grid chrome. The numbers count themselves in.
 */
export function Proof() {
  const root = useRef<HTMLElement>(null);

  useEffect(() => {
    const el = root.current;
    if (!el) return;
    const numbers = Array.from(el.querySelectorAll<HTMLElement>('[data-count]'));

    if (prefersReducedMotion()) {
      numbers.forEach((n) => (n.textContent = pad(Number(n.dataset.count ?? 0))));
      return;
    }

    let raf = 0;
    let started = false;
    const run = () => {
      const start = performance.now();
      const frame = (now: number) => {
        const t = Math.min(1, (now - start) / 1100);
        const eased = 1 - Math.pow(2, -9 * t);
        numbers.forEach((n) => {
          n.textContent = pad(Math.round(Number(n.dataset.count ?? 0) * eased));
        });
        if (t < 1) raf = requestAnimationFrame(frame);
      };
      raf = requestAnimationFrame(frame);
    };
    const io = new IntersectionObserver(
      (entries) => {
        if (!entries[0]?.isIntersecting || started) return;
        started = true;
        run();
        io.disconnect();
      },
      { threshold: 0.3 },
    );
    io.observe(el);
    return () => {
      io.disconnect();
      cancelAnimationFrame(raf);
    };
  }, []);

  return (
    <section ref={root} id="proof" className="proof" aria-label="The proof in numbers">
      <header className="shell">
        <span className="label">Chapter 03 — the proof</span>
      </header>
      <div className="proof__stack">
        {STATS.slice(0, 3).map((stat, i) => (
          <div className={`proof__row proof__row--${i}`} key={stat.label}>
            <span className="proof__num">
              <span data-count={stat.value}>{prefersReducedMotion() ? pad(stat.value) : '00'}</span>
            </span>
            <div className="proof__meta">
              <b>{stat.label}</b>
              <span>{stat.detail}</span>
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}
