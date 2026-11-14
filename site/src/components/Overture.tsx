import { useEffect } from 'react';
import { Reveal } from './Reveal';
import { getSignalEngine } from '../lib/useSignal';
import { Readouts } from './Readouts';
import { VERSION } from '../content/site';

/**
 * Chapter 1 — the void. A whisper of signal, a title, a promise.
 * The engine idles near-flat here; The Claim wakes it up.
 */
export function Overture() {
  useEffect(() => {
    getSignalEngine().gain = 0.12;
    return () => {
      getSignalEngine().gain = 1;
    };
  }, []);

  return (
    <section id="overture" className="overture" aria-label="Introduction">
      <div className="overture__mid">
        <Reveal as="h1" id="hero-title" className="overture__word" mode="chars" immediate>
          SauceFX
        </Reveal>
        <span className="overture__sub">v{VERSION} · the FxSound DSP engine, native</span>
      </div>

      <div className="overture__foot">
        <Readouts />
        <span className="overture__cue" aria-hidden="true" />
      </div>
    </section>
  );
}
