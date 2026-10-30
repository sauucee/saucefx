import { useEffect, useState } from 'react';
import { ScrollTrigger } from 'gsap/ScrollTrigger';
import { Nav } from './components/Nav';
import { Preloader, shouldPlayPreloader } from './components/Preloader';
import { Hero } from './components/Hero';
import { Stats } from './components/Stats';
import { Marquee } from './components/Marquee';
import { Rack } from './components/Rack';
import { EqPlayground } from './components/EqPlayground';
import { FeatureRail } from './components/FeatureRail';
import { Install } from './components/Install';
import { Footer } from './components/Footer';
import { ScrollProgress } from './components/ScrollProgress';
import { useSmoothScroll } from './lib/useSmoothScroll';

export default function App() {
  const [intro] = useState(shouldPlayPreloader);
  const [ready, setReady] = useState(() => !intro);
  const [curtainGone, setCurtainGone] = useState(() => !intro);
  useSmoothScroll();

  /**
   * Pins and scrubs are measured from the DOM, so every trigger is re-measured
   * once the screenshot and the self-hosted font have settled. Without this the
   * rack's pin spacer lands a few hundred pixels short on a cold load.
   */
  useEffect(() => {
    const refresh = () => ScrollTrigger.refresh();
    const timer = window.setTimeout(refresh, 140);

    document.fonts?.ready.then(refresh).catch(() => undefined);

    const image = document.querySelector<HTMLImageElement>('.hero__window img');
    if (image && !image.complete) image.addEventListener('load', refresh, { once: true });

    return () => window.clearTimeout(timer);
  }, [ready, curtainGone]);

  return (
    <>
      {!curtainGone && (
        <Preloader onReveal={() => setReady(true)} onDone={() => setCurtainGone(true)} />
      )}

      <ScrollProgress />

      <a className="skip-link" href="#main">
        Skip to content
      </a>

      <Nav />

      <main id="main">
        <Hero ready={ready} />
        <Stats />
        <Marquee />
        <Rack />
        <EqPlayground />
        <FeatureRail />
        <Install />
      </main>

      <Footer />
      <div className="grain" aria-hidden="true" />
    </>
  );
}
