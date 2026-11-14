import { useEffect, useRef } from 'react';
import { Minus, Square, X, RotateCcw, Settings2 } from 'lucide-react';
import { getSignalEngine } from '../lib/useSignal';
import { EFFECTS, VERSION } from '../content/site';
import logo from '../assets/saucefx-logo.svg';

const BANDS = 31;

/**
 * The product, rebuilt in code: crisp at any scale, and alive — the EQ
 * spectrum and response curve are driven by the same signal engine as the
 * rest of the page. Per-frame values are written through refs; no re-renders.
 */
export function AppWindow() {
  const bars = useRef<(HTMLElement | null)[]>([]);
  const curve = useRef<SVGPolylineElement>(null);
  const faders = useRef<(HTMLElement | null)[]>([]);

  useEffect(() => {
    let raf = 0;
    const loop = () => {
      const e = getSignalEngine();
      for (let i = 0; i < BANDS; i++) {
        const el = bars.current[i];
        if (el) el.style.transform = `scaleY(${(0.04 + e.bands[i] * 0.96).toFixed(3)})`;
      }
      // the response curve: a gentle S shaped by overall level
      const line = curve.current;
      if (line) {
        const pts: string[] = [];
        const lift = e.level * 26;
        for (let i = 0; i < BANDS; i++) {
          const x = (i / (BANDS - 1)) * 100;
          const y = 50 - Math.sin((i / (BANDS - 1)) * Math.PI) * lift;
          pts.push(`${x.toFixed(2)},${y.toFixed(2)}`);
        }
        line.setAttribute('points', pts.join(' '));
      }
      // the five faders breathe with the engine's character (decorative but live)
      faders.current.forEach((f, i) => {
        if (!f) return;
        const v = e.bands[Math.min(BANDS - 1, i * 6 + 2)];
        f.style.setProperty('--fd', (12 + v * 70).toFixed(1) + '%');
      });
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  }, []);

  return (
    <div className="appwin" role="img" aria-label={`The SauceFX ${VERSION} window: five effects and a 31-band equalizer`}>
      <div className="appwin__bar">
        <img src={logo} alt="" width={18} height={18} />
        <span className="appwin__name">
          SauceFX
          <small>AUDIO ENHANCER</small>
        </span>
        <span className="appwin__on label">ON</span>
        <span className="appwin__btns" aria-hidden="true">
          <Minus size={11} /> <Square size={9} /> <X size={11} />
        </span>
      </div>

      <div className="appwin__io">
        <div>
          <span className="label">Preset</span>
          <b>Default</b>
          <small>Factory preset</small>
        </div>
        <div>
          <span className="label">Output</span>
          <b>SauceFX</b>
          <small>Virtual output device</small>
        </div>
      </div>

      <div className="appwin__main">
        <div className="appwin__effects">
          <div className="appwin__panel-head">
            <b>Effects</b>
            <span className="label">Five native DSP controls</span>
          </div>
          {EFFECTS.map((effect, i) => (
            <div className="appwin__fx" key={effect}>
              <span className="appwin__fx-name">{effect}</span>
              <span className="appwin__fader" ref={(el) => { faders.current[i] = el; }} aria-hidden="true">
                <i className="appwin__fader-dot" />
              </span>
              <span className="appwin__fx-val">0</span>
            </div>
          ))}
        </div>

        <div className="appwin__eq">
          <div className="appwin__panel-head">
            <b>Equalizer</b>
            <span className="label">31 bands · ±12 dB</span>
          </div>
          <div className="appwin__spectrum" aria-hidden="true">
            {Array.from({ length: BANDS }, (_, i) => (
              <i
                key={i}
                ref={(el) => { bars.current[i] = el; }}
                style={{ left: `${(i / BANDS) * 100}%` }}
              />
            ))}
            <svg className="appwin__curve" viewBox="0 0 100 100" preserveAspectRatio="none">
              <polyline ref={curve} points="" />
            </svg>
          </div>
          <div className="appwin__scale label" aria-hidden="true">
            <b>20</b><b>60</b><b>250</b><b>1k</b><b>4k</b><b>12k</b><b>20k</b>
          </div>
        </div>
      </div>

      <div className="appwin__foot">
        <RotateCcw size={10} aria-hidden="true" /> Double-click a slider to reset it
        <span className="appwin__foot-right">
          <Settings2 size={10} aria-hidden="true" />
        </span>
      </div>
    </div>
  );
}
