import { useCallback, useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from 'react';
import {
  ArrowLeft,
  ArrowRight,
  Cable,
  History,
  MoveHorizontal,
  Power,
  SlidersHorizontal,
  Square,
} from 'lucide-react';
import { FEATURES } from '../content/site';

const ICONS = [SlidersHorizontal, Cable, History, Power, Square];

export function FeatureRail() {
  const rail = useRef<HTMLDivElement>(null);
  const drag = useRef({ active: false, startX: 0, left: 0 });
  const [dragging, setDragging] = useState(false);
  const [atStart, setAtStart] = useState(true);
  const [atEnd, setAtEnd] = useState(false);

  const sync = useCallback(() => {
    const el = rail.current;
    if (!el) return;
    setAtStart(el.scrollLeft < 4);
    setAtEnd(el.scrollLeft + el.clientWidth >= el.scrollWidth - 4);
  }, []);

  useEffect(() => {
    sync();
    const el = rail.current;
    if (!el) return;
    el.addEventListener('scroll', sync, { passive: true });
    window.addEventListener('resize', sync);
    return () => {
      el.removeEventListener('scroll', sync);
      window.removeEventListener('resize', sync);
    };
  }, [sync]);

  const nudge = (direction: 1 | -1) => {
    const el = rail.current;
    if (!el) return;
    const card = el.querySelector<HTMLElement>('.rail__card');
    const step = (card?.offsetWidth ?? 340) + 1;
    el.scrollBy({ left: step * direction, behavior: 'smooth' });
  };

  const onPointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.pointerType !== 'mouse' || event.button !== 0) return;
    const el = rail.current;
    if (!el) return;
    event.preventDefault();
    drag.current = { active: true, startX: event.clientX, left: el.scrollLeft };
    el.setPointerCapture(event.pointerId);
    setDragging(true);
  };

  const onPointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    const el = rail.current;
    if (!drag.current.active || !el) return;
    el.scrollLeft = drag.current.left - (event.clientX - drag.current.startX);
  };

  const endDrag = (event: ReactPointerEvent<HTMLDivElement>) => {
    const el = rail.current;
    drag.current.active = false;
    setDragging(false);
    if (el?.hasPointerCapture(event.pointerId)) el.releasePointerCapture(event.pointerId);
  };

  return (
    <section id="features" className="rail sec" aria-labelledby="rail-title">
      <div className="shell">
        <header className="sec__head">
          <span className="label">04 — In the app</span>
          <span className="label">Five things under the window</span>
        </header>
      </div>

      <div className="shell rail__head">
        <h2 id="rail-title" className="rail__title">
          Not a wrapper.
          <br />
          An instrument.
        </h2>
        <p className="sec__lede">
          The engine is native and the window is the app. Everything below is inside
          SauceFX — the site you are reading is the only thing that does not ship.
        </p>
      </div>

      <div
        className="rail__viewport"
        ref={rail}
        data-drag={dragging}
        role="region"
        aria-label="App features, scroll horizontally"
        tabIndex={0}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        onKeyDown={(event) => {
          if (event.key === 'ArrowRight') {
            event.preventDefault();
            nudge(1);
          } else if (event.key === 'ArrowLeft') {
            event.preventDefault();
            nudge(-1);
          }
        }}
      >
        <div className="rail__track">
          {FEATURES.map((feature, i) => {
            const Icon = ICONS[i] ?? Square;
            return (
              <article className="rail__card" key={feature.title}>
                <div className="rail__card-top">
                  <span className="rail__num">0{i + 1}</span>
                  <span className="rail__mark">
                    <Icon aria-hidden="true" strokeWidth={1.9} />
                  </span>
                </div>

                <div>
                  <h3>{feature.title}</h3>
                  <p>{feature.body}</p>
                  <small>{feature.detail}</small>
                </div>
              </article>
            );
          })}
        </div>
      </div>

      <div className="shell rail__controls">
        <span className="label">
          <MoveHorizontal size={12} aria-hidden="true" /> Drag or scroll
        </span>

        <div className="rail__arrows">
          <button
            type="button"
            className="icon-btn"
            aria-label="Previous feature"
            onClick={() => nudge(-1)}
            disabled={atStart}
          >
            <ArrowLeft size={15} aria-hidden="true" />
          </button>
          <button
            type="button"
            className="icon-btn"
            aria-label="Next feature"
            onClick={() => nudge(1)}
            disabled={atEnd}
          >
            <ArrowRight size={15} aria-hidden="true" />
          </button>
        </div>
      </div>
    </section>
  );
}
