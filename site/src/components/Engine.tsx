import { useRef } from 'react';
import gsap from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';
import { useGSAP } from '@gsap/react';
import { AppScreenshot } from './AppScreenshot';
import { Readouts } from './Readouts';
import { Reveal } from './Reveal';
import { REPO_URL } from '../content/site';
import { ArrowUpRight } from 'lucide-react';

gsap.registerPlugin(ScrollTrigger, useGSAP);

/**
 * Chapter 3 — the engine. The app rises like a monolith, bottom-anchored,
 * as the copy states the one thing that matters: native, not a wrapper.
 */
export function Engine() {
  const root = useRef<HTMLElement>(null);

  useGSAP(
    () => {
      const mm = gsap.matchMedia();
      mm.add('(prefers-reduced-motion: no-preference)', () => {
        gsap.fromTo(
          '.engine__window',
          { yPercent: 46 },
          {
            yPercent: 0,
            ease: 'none',
            scrollTrigger: {
              trigger: root.current,
              start: 'top 92%',
              end: 'top 8%',
              scrub: 0.5,
            },
          },
        );
        gsap.fromTo(
          '.engine__copy > *',
          { opacity: 0, y: 30 },
          {
            opacity: 1,
            y: 0,
            duration: 0.9,
            stagger: 0.12,
            ease: 'expo.out',
            scrollTrigger: { trigger: '.engine__copy', start: 'top 78%', once: true },
          },
        );
      });
      return () => mm.revert();
    },
    { scope: root },
  );

  return (
    <section ref={root} id="engine" className="engine" aria-labelledby="engine-title">
      <div className="shell engine__head">
        <span className="label">Chapter 01 — the engine</span>
      </div>

      <div className="shell engine__copy">
        <Reveal as="h2" id="engine-title" className="engine__title">
          Not a wrapper.
          <br />
          <em>The engine itself.</em>
        </Reveal>
        <p className="engine__body">
          SauceFX runs the open-source FxSound DSP engine natively on Linux and routes
          system audio through a PipeWire virtual sink. Five effects, a 31-band graphic
          EQ, one frameless window.
        </p>
        <a className="btn" href={REPO_URL} target="_blank" rel="noreferrer noopener">
          Read the source <ArrowUpRight aria-hidden="true" />
        </a>
      </div>

      <div className="engine__window">
        <AppScreenshot />
      </div>

      <div className="shell engine__readouts">
        <Readouts />
      </div>
    </section>
  );
}
