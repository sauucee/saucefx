import { useEffect, useRef } from 'react';
import { getSignalEngine } from '../lib/useSignal';
import { prefersReducedMotion } from '../lib/usePrefersReducedMotion';

/**
 * Live instrument strip: real numbers from the signal engine, written to the
 * DOM per frame through refs (no React re-renders). Static under reduced
 * motion / motion-off — the values still say everything.
 */
export function Readouts() {
  const time = useRef<HTMLSpanElement>(null);
  const peak = useRef<HTMLSpanElement>(null);
  const rms = useRef<HTMLSpanElement>(null);
  const mode = useRef<HTMLSpanElement>(null);

  useEffect(() => {
    if (prefersReducedMotion()) {
      if (peak.current) peak.current.textContent = '−∞ dB';
      if (rms.current) rms.current.textContent = '00%';
      return;
    }
    let raf = 0;
    const loop = () => {
      const e = getSignalEngine();
      const t = e.bands;
      let max = 0;
      let sum = 0;
      for (let i = 0; i < t.length; i++) {
        if (t[i] > max) max = t[i];
        sum += t[i];
      }
      const mean = sum / t.length;
      // 0..1 band energy onto a believable dBFS scale (-60..0).
      const db = max <= 0.001 ? '−∞' : `−${(60 - max * 60).toFixed(1)}`;
      const mm = Math.floor(e.time / 60);
      const ss = Math.floor(e.time % 60);
      const cs = Math.floor((e.time % 1) * 100);
      if (time.current)
        time.current.textContent = `${String(mm).padStart(2, '0')}:${String(ss).padStart(2, '0')}.${String(cs).padStart(2, '0')}`;
      if (peak.current) peak.current.textContent = `${db} dB`;
      if (rms.current) rms.current.textContent = `${String(Math.round(mean * 100)).padStart(2, '0')}%`;
      if (mode.current) mode.current.textContent = e.mode === 'mic' ? '● MIC' : '▶ DEMO';
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  }, []);

  return (
    <div className="readouts label" aria-hidden="true">
      <span className="readouts__cell">
        SIG <span ref={time}>00:00.00</span>
      </span>
      <span className="readouts__cell">
        PEAK <span ref={peak}>−∞ dB</span>
      </span>
      <span className="readouts__cell">
        RMS <span ref={rms}>00%</span>
      </span>
      <span className="readouts__cell">
        31 BAND
      </span>
      <span className="readouts__cell readouts__cell--mode" ref={mode}>
        ▶ DEMO
      </span>
    </div>
  );
}
