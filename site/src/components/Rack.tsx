import { useEffect, useRef, useState } from 'react';
import { useGSAP } from '@gsap/react';
import { gsap } from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';
import { EFFECTS, EFFECT_NOTES } from '../content/site';
import { prefersReducedMotion } from '../lib/usePrefersReducedMotion';
import { useMediaQuery } from '../lib/useMediaQuery';

gsap.registerPlugin(ScrollTrigger);

const LEVELS = [
  [0.92, 0.32, 0.28, 0.26, 0.24],
  [0.3, 0.86, 0.34, 0.28, 0.3],
  [0.34, 0.3, 0.9, 0.4, 0.28],
  [0.28, 0.3, 0.36, 0.94, 0.42],
  [0.4, 0.32, 0.3, 0.44, 0.97],
];

const SHORT = ['CLR', 'AMB', 'SRR', 'DYN', 'BASS'];

const TAGS = [
  'Presence / 2–6 kHz',
  'Space / decay tail',
  'Width / L–R',
  'Dynamics / ceiling',
  'Weight / low shelf',
];

const BASE = 150;

/**
 * Five small response drawings. They are diagrams of what each fader does to
 * the curve — decorative, not a measurement of the app's DSP.
 */
function Plot({ index }: { index: number }) {
  const stroke = 'var(--accent-text)';

  if (index === 0) {
    const line =
      'M12 134 L58 132 C88 130 96 118 108 92 C120 64 134 42 160 42 C186 42 200 64 212 92 C224 118 232 130 262 132 L308 134';
    return (
      <>
        <path d={`${line} L308 168 L12 168 Z`} fill="var(--accent)" fillOpacity="0.08" />
        <path d={line} stroke={stroke} strokeWidth="2" fill="none" strokeLinecap="round" />
        <line x1="160" y1="26" x2="160" y2={BASE + 14} stroke="var(--line)" strokeDasharray="2 6" />
        {[
          [108, 92],
          [160, 42],
          [212, 92],
        ].map(([x, y]) => (
          <circle key={x} cx={x} cy={y} r="3" fill={stroke} />
        ))}
        <text x="12" y="16" fill="var(--muted)">
          PRESENCE / 2–6 KHZ
        </text>
        <text x="308" y="16" textAnchor="end" fill="var(--muted)">
          +12
        </text>
      </>
    );
  }

  if (index === 1) {
    const envelope = 'M22 62 C70 92 108 122 158 136 C210 150 254 150 306 150';
    return (
      <>
        {[
          [22, 62],
          [58, 96],
          [96, 114],
          [140, 128],
          [188, 138],
        ].map(([x, top], i) => (
          <rect
            key={x}
            x={x}
            y={top}
            width={i === 0 ? 7 : 4}
            height={BASE - top}
            fill={stroke}
            opacity={0.9 - i * 0.14}
          />
        ))}
        <path d={envelope} stroke="var(--line-strong)" strokeWidth="1" fill="none" strokeDasharray="2 5" />
        <line x1="12" y1={BASE} x2="308" y2={BASE} stroke="var(--line-strong)" />
        <circle cx="22" cy="62" r="3" fill={stroke} />
        <text x="12" y="16" fill="var(--muted)">
          SPACE / DECAY TAIL
        </text>
        <text x="308" y="16" textAnchor="end" fill="var(--muted)">
          ~ 1.2 s
        </text>
      </>
    );
  }

  if (index === 2) {
    return (
      <>
        <line x1="160" y1="20" x2="160" y2="164" stroke="var(--line)" strokeDasharray="2 6" />
        <path
          d="M14 150 C58 150 68 58 106 58 C148 58 156 150 160 150"
          stroke={stroke}
          strokeWidth="2"
          fill="var(--accent)"
          fillOpacity="0.07"
        />
        <path
          d="M306 150 C262 150 252 58 214 58 C172 58 164 150 160 150"
          stroke={stroke}
          strokeWidth="2"
          fill="var(--accent)"
          fillOpacity="0.07"
        />
        <circle cx="106" cy="58" r="3" fill={stroke} />
        <circle cx="214" cy="58" r="3" fill={stroke} />
        <line x1="106" y1="34" x2="214" y2="34" stroke="var(--line-strong)" />
        <path d="M106 34l7 -3v6Zm108 0l-7 -3v6Z" fill="var(--line-strong)" />
        <text x="160" y="26" textAnchor="middle" fill="var(--muted)">
          WIDTH
        </text>
        <text x="12" y="16" fill="var(--muted)">
          WIDTH / L–R
        </text>
      </>
    );
  }

  if (index === 3) {
    const wave =
      'M14 118 L30 118 L36 96 L42 96 L48 118 L62 118 L68 78 L74 78 L80 118 L94 118 L100 64 L108 64 L114 118 L130 118 L136 70 L144 70 L150 118 L166 118 L172 74 L180 74 L186 118 L202 118 L208 66 L216 66 L222 118 L238 118 L244 82 L252 82 L258 118 L274 118 L280 92 L288 92 L294 118 L306 118';
    return (
      <>
        <line x1="12" y1="52" x2="308" y2="52" stroke="var(--accent-line)" strokeDasharray="4 6" />
        <line x1="12" y1="120" x2="308" y2="120" stroke="var(--line)" strokeDasharray="2 7" />
        <path d={wave} stroke={stroke} strokeWidth="2" fill="none" strokeLinejoin="round" />
        <path d="M14 52h294" stroke="var(--line-soft)" strokeDasharray="1 4" />
        <text x="12" y="16" fill="var(--muted)">
          DYNAMICS / CEILING
        </text>
        <text x="308" y="16" textAnchor="end" fill="var(--muted)">
          −0 dB
        </text>
      </>
    );
  }

  return (
    <>
      {[22, 120, 168, 206, 236, 262].map((x, i) => {
        const h = [92, 74, 54, 40, 30, 22][i];
        return (
          <rect key={x} x={x} y={BASE - h} width="12" height={h} fill="var(--accent)" opacity={0.85 - i * 0.12} />
        );
      })}
      <path
        d="M14 58 C58 58 74 64 92 116 C104 146 118 150 158 150 L306 150"
        stroke={stroke}
        strokeWidth="2"
        fill="var(--accent)"
        fillOpacity="0.08"
      />
      <line x1="12" y1="166" x2="308" y2="166" stroke="var(--line-strong)" />
      <text x="12" y="16" fill="var(--muted)">
        WEIGHT / LOW SHELF
      </text>
      <text x="308" y="16" textAnchor="end" fill="var(--muted)">
        +12
      </text>
    </>
  );
}

function RackStage({ index }: { index: number }) {
  return (
    <div className="rack__stage">
      <div className="rack__panel">
        <div className="rack__panel-head">
          <span className="label">
            <b>{`0${index + 1}`}</b> {TAGS[index]} / response
          </span>
          <span className="label live">Signal</span>
        </div>

        <div className="rack__plots" aria-hidden="true">
          {EFFECTS.map((effect, i) => (
            <div className="rack__plot" data-active={i === index} key={effect}>
              <svg viewBox="0 0 320 180" fill="none" aria-hidden="true">
                <Plot index={i} />
              </svg>
            </div>
          ))}
        </div>

        <div className="rack__axis">
          <span className="label">20 Hz</span>
          <span className="label">200</span>
          <span className="label">2 k</span>
          <span className="label">20 kHz</span>
        </div>
      </div>

      <div className="rack__faders" aria-hidden="true">
        {EFFECTS.map((effect, i) => (
          <div className="rack__fader" data-active={i === index} key={effect}>
            <div className="rack__fader-bar">
              <span style={{ ['--level' as string]: `${Math.round(LEVELS[index][i] * 100)}%` }} />
            </div>
            <span>{SHORT[i]}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

export function Rack() {
  const reduced = prefersReducedMotion();
  const wide = useMediaQuery('(min-width: 900px)');
  const canPin = wide && !reduced;
  const [index, setIndex] = useState(0);
  const section = useRef<HTMLElement>(null);
  const pin = useRef<HTMLDivElement>(null);
  const trigger = useRef<ScrollTrigger | null>(null);
  const segments = useRef<Array<HTMLElement | null>>([]);

  useGSAP(
    () => {
      if (!canPin) return;

      const instance = ScrollTrigger.create({
        trigger: section.current,
        start: 'top top',
        end: '+=320%',
        pin: pin.current,
        pinSpacing: true,
        anticipatePin: 1,
        invalidateOnRefresh: true,
        onUpdate: (self) => {
          const scaled = self.progress * EFFECTS.length;
          const next = Math.min(EFFECTS.length - 1, Math.floor(scaled));
          setIndex((prev) => (prev === next ? prev : next));

          segments.current.forEach((segment, i) => {
            const fill = Math.min(1, Math.max(0, scaled - i));
            segment?.style.setProperty('--fill', fill.toFixed(3));
          });
        },
      });
      trigger.current = instance;

      const raf = requestAnimationFrame(() => ScrollTrigger.refresh());
      return () => {
        cancelAnimationFrame(raf);
        trigger.current = null;
        instance.kill();
      };
    },
    { scope: section, dependencies: [canPin] },
  );

  useEffect(() => {
    if (canPin) return;
    const onScroll = () => {
      const root = section.current;
      if (!root) return;
      const rect = root.getBoundingClientRect();
      const progress = Math.min(1, Math.max(0, -rect.top / Math.max(1, rect.height - window.innerHeight)));
      setIndex(Math.min(EFFECTS.length - 1, Math.floor(progress * EFFECTS.length)));
    };
    onScroll();
    window.addEventListener('scroll', onScroll, { passive: true });
    return () => window.removeEventListener('scroll', onScroll);
  }, [canPin]);

  const goto = (i: number) => {
    const instance = trigger.current;
    if (!instance) return;
    const top = instance.start + (i / (EFFECTS.length - 1)) * (instance.end - instance.start);
    window.scrollTo({ top: top + 2, behavior: 'smooth' });
  };

  if (!canPin) {
    return (
      <section id="rack" className="rack sec" ref={section} aria-labelledby="rack-title">
        <div className="shell rack-list">
          <header className="sec__head">
            <span className="label">02 — The rack</span>
            <span className="label">05 faders / one chain</span>
          </header>

          <div>
            <h2 id="rack-title" className="sec__title">
              Five effects.
              <br />
              One chain.
            </h2>
            <p className="sec__lede" style={{ marginTop: '1rem' }}>
              Each fader is one stage of the chain. Nudge them in order, or jump straight to the
              band that is bothering you.
            </p>
          </div>

          <ul className="rack-list__items">
            {EFFECTS.map((effect, i) => (
              <li className="rack-list__item" key={effect}>
                <span className="rack-list__num">0{i + 1}</span>
                <div>
                  <h3>{effect}</h3>
                  <p>{EFFECT_NOTES[effect]}</p>
                </div>
              </li>
            ))}
          </ul>
        </div>
      </section>
    );
  }

  return (
    <section id="rack" ref={section} className="rack" aria-labelledby="rack-title">
      <div ref={pin} className="rack__pin">
        <div className="shell rack__in">
          <header className="sec__head">
            <span className="label">02 — The rack</span>
            <span className="label">Scroll to walk the chain</span>
          </header>

          <div className="rack__body">
            <div className="rack__copy">
              <div className="rack__idx">
                0{index + 1}
                <span>of 05</span>
              </div>

              <h2 className="rack__title" id="rack-title" key={index}>
                {EFFECTS[index]}
              </h2>

              <p className="rack__note">{EFFECT_NOTES[EFFECTS[index]]}</p>

              <div className="rack__ticks">
                {EFFECTS.map((effect, i) => (
                  <button
                    key={effect}
                    type="button"
                    className="rack__tick"
                    aria-current={i === index}
                    onClick={() => goto(i)}
                  >
                    <span>0{i + 1}</span>
                    {effect}
                  </button>
                ))}
              </div>
            </div>

            <RackStage index={index} />
          </div>

          <div className="rack__progress">
            {EFFECTS.map((effect, i) => (
              <i key={effect}>
                <b
                  ref={(el) => {
                    segments.current[i] = el;
                  }}
                />
              </i>
            ))}
            <span className="label rack__count">0{index + 1} / 05</span>
          </div>

          <ul className="sr-only">
            {EFFECTS.map((effect) => (
              <li key={effect}>
                {effect}: {EFFECT_NOTES[effect]}
              </li>
            ))}
          </ul>
        </div>
      </div>
    </section>
  );
}
