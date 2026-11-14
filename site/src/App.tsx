import { lazy, Suspense, useState, useEffect } from 'react';
import { ScrollTrigger } from 'gsap/ScrollTrigger';
import { Nav } from './components/Nav';
import { Preloader, shouldPlayPreloader } from './components/Preloader';
import { Overture } from './components/Overture';
import { Claim } from './components/Claim';
import { Engine } from './components/Engine';
import { Chain } from './components/Chain';
import { EqPlayground } from './components/EqPlayground';
import { Proof } from './components/Proof';
import { InstallTerminal } from './components/InstallTerminal';
import { Outro } from './components/Outro';
import { Transport } from './components/Transport';
import { useSmoothScroll } from './lib/useSmoothScroll';
import { usePrefersReducedMotion } from './lib/usePrefersReducedMotion';
import { useSignalDriver } from './lib/useSignal';
import { webglSupported } from './lib/webgl';

const SignalFieldLazy = lazy(() => import('./three/SignalField'));

export default function App() {
  const [intro] = useState(shouldPlayPreloader);
  const [ready, setReady] = useState(() => !intro);
  const [curtainGone, setCurtainGone] = useState(() => !intro);
  const reduced = usePrefersReducedMotion();
  const [canWebGL] = useState(() => webglSupported());
  const signal = !reduced && canWebGL;
  useSmoothScroll();
  useSignalDriver(!reduced);

  /**
   * Pins and scrubs are measured from the DOM — re-measure once the font
   * and the screenshot have settled, or the pin spacers land short.
   */
  useEffect(() => {
    const refresh = () => ScrollTrigger.refresh();
    const timer = window.setTimeout(refresh, 140);
    document.fonts?.ready.then(refresh).catch(() => undefined);
    const image = document.querySelector<HTMLImageElement>('.engine__window img');
    if (image && !image.complete) image.addEventListener('load', refresh, { once: true });
    return () => window.clearTimeout(timer);
  }, [ready, curtainGone]);

  return (
    <>
      {!curtainGone && (
        <Preloader onReveal={() => setReady(true)} onDone={() => setCurtainGone(true)} />
      )}

      {signal && (
        <Suspense fallback={null}>
          <SignalFieldLazy />
        </Suspense>
      )}

      <Transport />

      <a className="skip-link" href="#main">
        Skip to content
      </a>

      <Nav />

      <main id="main">
        <Overture />
        <Claim />
        <Engine />
        <Chain />
        <EqPlayground />
        <Proof />
        <InstallTerminal />
      </main>

      <Outro />
      <div className="grain" aria-hidden="true" />
    </>
  );
}
