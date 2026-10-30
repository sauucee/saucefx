import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from 'react';
import { RotateCcw } from 'lucide-react';
import { BANDS, formatHz, smoothPath } from '../lib/curve';
import { prefersReducedMotion } from '../lib/usePrefersReducedMotion';

const BAND_COUNT = BANDS.length;

const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));

/** Build a curve from gaussian bumps so each demo shape is readable, not noise. */
function shape(bumps: Array<[center: number, width: number, gain: number]>): number[] {
  return Array.from({ length: BAND_COUNT }, (_, i) =>
    clamp(
      Math.round(bumps.reduce((sum, [center, width, gain]) => sum + gain * Math.exp(-((i - center) ** 2) / (2 * width * width)), 0)),
      -12,
      12,
    ),
  );
}

const SHAPES: Record<string, number[]> = {
  Default: Array.from({ length: BAND_COUNT }, () => 0),
  Music: shape([
    [1, 2.2, 4],
    [6, 3, 1.5],
    [19, 2.4, 3],
    [26, 2.6, 2],
  ]),
  Bass: shape([
    [1, 2.4, 8],
    [6, 3.2, 3],
    [18, 2.6, -1.5],
  ]),
  Voice: shape([
    [11, 2.6, -3],
    [20, 2.2, 4],
    [26, 2.4, 2],
    [4, 2.4, -1],
  ]),
  Wide: shape([
    [2, 2.6, 2],
    [14, 3, -2],
    [24, 3, 4],
    [29, 2, 3],
  ]),
};

const PRESETS = Object.keys(SHAPES);
const START = 'Default';

/** Gain (−12…12) as a percentage from the top of the plot. */
const yFor = (gain: number) => 50 - (gain / 12) * 46;

export function EqPlayground() {
  const [gains, setGains] = useState<number[]>(SHAPES[START]);
  const [preset, setPreset] = useState<string | null>(START);
  const [liveBand, setLiveBand] = useState<number | null>(null);
  const [dragging, setDragging] = useState(false);

  const plotRef = useRef<HTMLDivElement>(null);
  const draggingRef = useRef(false);
  const gainsRef = useRef(gains);
  const rafRef = useRef(0);
  gainsRef.current = gains;

  useEffect(() => {
    return () => {
      if (rafRef.current) cancelAnimationFrame(rafRef.current);
    };
  }, []);

  const applyPointer = (clientX: number, clientY: number) => {
    const plot = plotRef.current;
    if (!plot) return;
    const rect = plot.getBoundingClientRect();
    const relX = clamp((clientX - rect.left) / rect.width, 0, 0.9999);
    const relY = clamp((clientY - rect.top) / rect.height, 0, 1);
    const band = Math.min(BAND_COUNT - 1, Math.floor(relX * BAND_COUNT));
    const gain = clamp(Math.round((0.5 - relY) * 24), -12, 12);

    setGains((prev) => {
      if (prev[band] === gain) return prev;
      const next = prev.slice();
      next[band] = gain;
      return next;
    });
    setLiveBand(band);
  };

  const onPointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    event.preventDefault();
    draggingRef.current = true;
    setDragging(true);
    setPreset(null);
    plotRef.current?.setPointerCapture(event.pointerId);
    applyPointer(event.clientX, event.clientY);
  };

  const onPointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (!draggingRef.current) return;
    applyPointer(event.clientX, event.clientY);
  };

  const endDrag = (event: ReactPointerEvent<HTMLDivElement>) => {
    draggingRef.current = false;
    setDragging(false);
    setLiveBand(null);
    if (plotRef.current?.hasPointerCapture(event.pointerId)) {
      plotRef.current.releasePointerCapture(event.pointerId);
    }
  };

  const animateTo = (target: number[], name: string | null) => {
    setPreset(name);
    if (rafRef.current) cancelAnimationFrame(rafRef.current);

    if (prefersReducedMotion()) {
      setGains(target);
      return;
    }

    const from = gainsRef.current.slice();
    const start = performance.now();
    const tick = (now: number) => {
      const t = Math.min(1, (now - start) / 460);
      const eased = 1 - Math.pow(1 - t, 3);
      setGains(from.map((value, i) => Math.round(value + (target[i] - value) * eased)));
      if (t < 1) rafRef.current = requestAnimationFrame(tick);
    };
    rafRef.current = requestAnimationFrame(tick);
  };

  const nudge = (index: number, delta: number) => {
    setPreset(null);
    setGains((prev) => {
      const next = prev.slice();
      next[index] = clamp(next[index] + delta, -12, 12);
      return next;
    });
    setLiveBand(index);
  };

  const points = gains.map((gain, i) => ({
    x: ((i + 0.5) / BAND_COUNT) * 100,
    y: yFor(gain),
  }));
  const line = smoothPath(points, 0.8);

  return (
    <section id="playground" className="eq sec" aria-labelledby="eq-title">
      <div className="shell">
        <header className="sec__head">
          <span className="label">03 — Playground</span>
          <span className="label">Mouse · touch · arrow keys</span>
        </header>

        <div className="eq__head">
          <h2 id="eq-title" className="eq__title">
            Thirty-one bands.
            <br />
            <em>Draw your own curve.</em>
          </h2>
          <p className="eq__lede">
            The same graphic layout as the app: pull a band up or down, or pick a shape and
            watch the curve settle. Nothing here is connected to audio — it is a drawing
            surface, not a preview.
          </p>
        </div>

        <div className="eq__panel">
          <div className="eq__bar">
            <div className="eq__bar-l">
              <span className="label">
                <span className="dot" /> Graphic EQ
              </span>
              <span className="label">31 bands / ±12 dB</span>
            </div>

            <div className="eq__chips" role="group" aria-label="Demo curve shapes">
              {PRESETS.map((name) => (
                <button
                  key={name}
                  type="button"
                  className="chip"
                  aria-pressed={preset === name}
                  onClick={() => animateTo(SHAPES[name], name)}
                >
                  {name}
                </button>
              ))}
            </div>
          </div>

          <div
            className="eq__plot"
            ref={plotRef}
            data-dragging={dragging}
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={endDrag}
            onPointerCancel={endDrag}
            onPointerLeave={() => {
              if (!draggingRef.current) setLiveBand(null);
            }}
            aria-describedby="eq-hint"
          >
            <div className="eq__spectrum" aria-hidden="true">
              {gains.map((gain, i) => (
                <span
                  key={i}
                  style={{
                    ['--h' as string]: `${18 + ((gain + 12) / 24) * 62}%`,
                    ['--d' as string]: `${(1.4 + (i % 7) * 0.22).toFixed(2)}s`,
                    ['--del' as string]: `${(((i * 29) % 100) / 100).toFixed(2)}s`,
                  }}
                />
              ))}
            </div>

            <svg
              className="eq__curve"
              viewBox="0 0 100 100"
              preserveAspectRatio="none"
              aria-hidden="true"
            >
              <path d={`${line} L 100 100 L 0 100 Z`} fill="var(--accent)" fillOpacity="0.1" />
              <path
                d={line}
                data-curve
                fill="none"
                stroke="var(--accent)"
                strokeWidth="2"
                vectorEffect="non-scaling-stroke"
                strokeLinejoin="round"
                strokeLinecap="round"
              />
            </svg>

            <div className="eq__bands">
              {gains.map((gain, i) => (
                <div className="eq__band" key={BANDS[i]} data-live={liveBand === i}>
                  <button
                    type="button"
                    className="eq__knob"
                    role="slider"
                    aria-label={`${formatHz(BANDS[i])} hertz band`}
                    aria-valuemin={-12}
                    aria-valuemax={12}
                    aria-valuenow={gain}
                    aria-valuetext={`${gain > 0 ? '+' : ''}${gain} decibels`}
                    tabIndex={0}
                    style={{ ['--y' as string]: `${yFor(gain)}%` }}
                    onKeyDown={(event) => {
                      if (event.key === 'ArrowUp' || event.key === 'ArrowRight') {
                        event.preventDefault();
                        nudge(i, 1);
                      } else if (event.key === 'ArrowDown' || event.key === 'ArrowLeft') {
                        event.preventDefault();
                        nudge(i, -1);
                      } else if (event.key === 'PageUp') {
                        event.preventDefault();
                        nudge(i, 3);
                      } else if (event.key === 'PageDown') {
                        event.preventDefault();
                        nudge(i, -3);
                      } else if (event.key === 'Home') {
                        event.preventDefault();
                        nudge(i, -gain);
                      }
                    }}
                    onFocus={() => setLiveBand(i)}
                    onBlur={() => setLiveBand(null)}
                  />
                </div>
              ))}
            </div>
          </div>

          <div className="eq__foot">
            <div className="eq__hz" aria-hidden="true">
              {[0, 6, 10, 14, 18, 22, 26, 30].map((index) => (
                <span key={BANDS[index]}>{formatHz(BANDS[index])}</span>
              ))}
            </div>

            <span className="label eq__hint" id="eq-hint">
              Drag any band · arrows to nudge
            </span>

            <button type="button" className="chip" onClick={() => animateTo(SHAPES.Default, 'Default')}>
              <RotateCcw size={11} aria-hidden="true" /> Reset
            </button>
          </div>
        </div>
      </div>
    </section>
  );
}
