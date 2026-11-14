import { useEffect, useRef } from 'react';

const TOTAL_SECONDS = 133; // the page as a ~2-minute "track"

const fmt = (s: number) =>
  `${String(Math.floor(s / 60)).padStart(2, '0')}:${String(Math.floor(s % 60)).padStart(2, '0')}`;

/**
 * Transport playhead: scroll position mapped onto a track timecode with a
 * playhead marker. Written straight to the DOM on scroll (rAF-coalesced).
 */
export function Transport() {
  const bar = useRef<HTMLSpanElement>(null);
  const head = useRef<HTMLSpanElement>(null);
  const time = useRef<HTMLSpanElement>(null);

  useEffect(() => {
    let frame = 0;

    const paint = () => {
      frame = 0;
      const max = document.documentElement.scrollHeight - window.innerHeight;
      const y = window.scrollY;
      const p = max > 0 ? Math.min(1, Math.max(0, y / max)) : 0;
      if (bar.current) bar.current.style.transform = `scaleX(${p.toFixed(4)})`;
      if (head.current) head.current.style.left = `${(p * 100).toFixed(2)}%`;
      if (time.current) time.current.textContent = fmt(p * TOTAL_SECONDS);
      // Only once the hero is behind you — nothing fixed overlaps the intro.
      document.documentElement.dataset.transport = y > window.innerHeight * 0.9 ? 'on' : 'off';
    };

    const onScroll = () => {
      if (!frame) frame = requestAnimationFrame(paint);
    };

    paint();
    window.addEventListener('scroll', onScroll, { passive: true });
    window.addEventListener('resize', onScroll);
    return () => {
      if (frame) cancelAnimationFrame(frame);
      window.removeEventListener('scroll', onScroll);
      window.removeEventListener('resize', onScroll);
    };
  }, []);

  return (
    <div className="transport label" aria-hidden="true">
      <span className="transport__glyph">▶</span>
      <span ref={time} className="transport__time">
        00:00
      </span>
      <span className="transport__rail">
        <span ref={bar} className="transport__bar" />
        <span ref={head} className="transport__head" />
      </span>
      <span className="transport__total">{fmt(TOTAL_SECONDS)}</span>
    </div>
  );
}
