import { useEffect, useRef } from 'react';
import { useGSAP } from '@gsap/react';
import { gsap } from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';
import { ArrowDown, ArrowUpRight, Github, Move } from 'lucide-react';
import { AppScreenshot } from './AppScreenshot';
import { Spectrum } from './Spectrum';
import { REPO_URL, VERSION } from '../content/site';
import { prefersReducedMotion } from '../lib/usePrefersReducedMotion';

gsap.registerPlugin(ScrollTrigger);

export function Hero({ ready }: { ready: boolean }) {
  const section = useRef<HTMLElement>(null);
  const stage = useRef<HTMLDivElement>(null);
  const word = useRef<HTMLHeadingElement>(null);
  const wordLayer = useRef<HTMLDivElement>(null);
  const windowLayer = useRef<HTMLDivElement>(null);
  const dragLayer = useRef<HTMLDivElement>(null);
  const tiltLayer = useRef<HTMLDivElement>(null);
  const hint = useRef<HTMLDivElement>(null);
  const intro = useRef<gsap.core.Timeline | null>(null);

  useGSAP(
    () => {
      const reduced = prefersReducedMotion();
      const wordEl = word.current;
      const winWrap = windowLayer.current;
      const tiltEl = tiltLayer.current;
      const dragEl = dragLayer.current;
      if (!wordEl || !winWrap || !tiltEl || !dragEl) return;

      if (reduced) {
        gsap.set([wordEl, winWrap, dragEl, tiltEl], { clearProps: 'all' });
        return;
      }

      const frameEl = section.current?.querySelector<HTMLElement>('.shot');

      const timeline = gsap.timeline({ paused: true, defaults: { ease: 'expo.out' } });
      timeline.fromTo(wordEl, { yPercent: 16, opacity: 0 }, { yPercent: 0, opacity: 1, duration: 1.3 });
      timeline.fromTo(
        frameEl ?? winWrap,
        { y: 64, opacity: 0, rotateX: 10 },
        { y: 0, opacity: 1, rotateX: 0, duration: 1.45 },
        0.08,
      );
      timeline.fromTo(
        '.hero__top > .label',
        { y: 14, opacity: 0 },
        { y: 0, opacity: 1, duration: 0.9, stagger: 0.06 },
        0.2,
      );
      timeline.fromTo(
        '.hero__foot > *',
        { y: 22, opacity: 0 },
        { y: 0, opacity: 1, duration: 0.95, stagger: 0.07 },
        0.36,
      );
      timeline.fromTo('.hero__spectrum', { opacity: 0 }, { opacity: 1, duration: 1.1 }, 0.3);
      intro.current = timeline;

      /* Scroll depth: word and window separate as the section leaves. */
      gsap.to(wordEl, {
        yPercent: -13,
        ease: 'none',
        scrollTrigger: { trigger: section.current, start: 'top top', end: 'bottom top', scrub: true },
      });
      gsap.to(winWrap, {
        yPercent: 10,
        ease: 'none',
        scrollTrigger: { trigger: section.current, start: 'top top', end: 'bottom top', scrub: true },
      });

      if (!window.matchMedia('(pointer: fine)').matches) return;

      const rotY = gsap.quickTo(tiltEl, 'rotationY', { duration: 0.9, ease: 'power3.out' });
      const rotX = gsap.quickTo(tiltEl, 'rotationX', { duration: 0.9, ease: 'power3.out' });
      const wordX = gsap.quickTo(wordEl, 'x', { duration: 1.2, ease: 'power3.out' });
      const wordY = gsap.quickTo(wordEl, 'y', { duration: 1.2, ease: 'power3.out' });
      const hintX = gsap.quickTo(hint.current, 'x', { duration: 0.4, ease: 'power3.out' });
      const hintY = gsap.quickTo(hint.current, 'y', { duration: 0.4, ease: 'power3.out' });

      let dragging = false;
      let dragStart = { x: 0, y: 0 };
      let pointerStart = { x: 0, y: 0 };
      let hintVisible = false;

      const onMove = (event: PointerEvent) => {
        const rect = stage.current?.getBoundingClientRect();
        if (!rect) return;
        const nx = ((event.clientX - rect.left) / rect.width) * 2 - 1;
        const ny = ((event.clientY - rect.top) / rect.height) * 2 - 1;

        if (!dragging) {
          rotY(nx * 7);
          rotX(-ny * 5);
          wordX(nx * -20);
          wordY(ny * -12);
        }

        if (hint.current && !dragging) {
          hintX(event.clientX - rect.left);
          hintY(event.clientY - rect.top);
          if (!hintVisible) {
            hintVisible = true;
            gsap.to(hint.current, { autoAlpha: 1, duration: 0.2 });
          }
        }

        if (dragging) {
          const limit = rect.width * 0.15;
          const dx = gsap.utils.clamp(-limit, limit, event.clientX - pointerStart.x);
          const dy = gsap.utils.clamp(-limit * 0.6, limit * 0.6, event.clientY - pointerStart.y);
          gsap.set(dragEl, { x: dragStart.x + dx, y: dragStart.y + dy });
        }
      };

      const hideHint = () => {
        hintVisible = false;
        if (hint.current) gsap.to(hint.current, { autoAlpha: 0, duration: 0.18 });
      };

      const onLeave = () => {
        rotY(0);
        rotX(0);
        wordX(0);
        wordY(0);
        hideHint();
      };

      const onDown = (event: PointerEvent) => {
        if (event.pointerType !== 'mouse' || event.button !== 0) return;
        event.preventDefault();
        dragging = true;
        dragEl.dataset.dragging = 'true';
        pointerStart = { x: event.clientX, y: event.clientY };
        dragStart = {
          x: gsap.getProperty(dragEl, 'x') as number,
          y: gsap.getProperty(dragEl, 'y') as number,
        };
        dragEl.setPointerCapture(event.pointerId);
        hideHint();
      };

      const onUp = (event: PointerEvent) => {
        if (!dragging) return;
        dragging = false;
        delete dragEl.dataset.dragging;
        if (dragEl.hasPointerCapture(event.pointerId)) dragEl.releasePointerCapture(event.pointerId);
        gsap.to(dragEl, { x: 0, y: 0, duration: 1.1, ease: 'elastic.out(1, 0.6)' });
        rotX(0);
        rotY(0);
      };

      stage.current?.addEventListener('pointermove', onMove);
      stage.current?.addEventListener('pointerleave', onLeave);
      dragEl.addEventListener('pointerdown', onDown);
      dragEl.addEventListener('pointerup', onUp);
      dragEl.addEventListener('pointercancel', onUp);

      return () => {
        stage.current?.removeEventListener('pointermove', onMove);
        stage.current?.removeEventListener('pointerleave', onLeave);
        dragEl.removeEventListener('pointerdown', onDown);
        dragEl.removeEventListener('pointerup', onUp);
        dragEl.removeEventListener('pointercancel', onUp);
      };
    },
    { scope: section },
  );

  useEffect(() => {
    if (!ready) return;
    const timer = window.setTimeout(() => intro.current?.play(), prefersReducedMotion() ? 0 : 60);
    return () => window.clearTimeout(timer);
  }, [ready]);

  return (
    <section id="top" className="hero" ref={section} aria-labelledby="hero-title">
      <div className="hero__rules" aria-hidden="true">
        {Array.from({ length: 5 }, (_, i) => (
          <span key={i} />
        ))}
      </div>
      <div className="hero__glow" aria-hidden="true" />

      <div className="shell hero__top">
        <span className="label">
          <span className="dot" /> SauceFX — v{VERSION}
        </span>
        <span className="label hero__top-mid">Sound EQ for Linux</span>
        <span className="label">DfxDsp engine · PipeWire</span>
      </div>

      <div className="hero__stage" ref={stage}>
        <div className="hero__word-layer" ref={wordLayer}>
          <span className="hero__side hero__side--l" aria-hidden="true">
            31 bands / 20 Hz — 20 kHz
          </span>
          <span className="hero__side hero__side--r" aria-hidden="true">
            Virtual sink / Native engine
          </span>
          <h1 id="hero-title" className="hero__word" ref={word}>
            SauceFX
          </h1>
        </div>

        <div className="hero__window-layer" ref={windowLayer}>
          <div ref={dragLayer} className="hero__window">
            <div ref={tiltLayer} className="hero__tilt">
              <AppScreenshot />
            </div>
          </div>
        </div>

        <div ref={hint} className="hero__cursor" aria-hidden="true">
          <Move size={10} strokeWidth={2.6} />
          drag
        </div>
      </div>

      <div className="shell hero__foot">
        <div>
          <p className="hero__sub">
            Sound EQ for <em>Linux.</em>
          </p>
          <p className="hero__text">
            The open-source FxSound DSP engine, native on Linux. A five-effect rack, a
            31-band graphic EQ and a PipeWire virtual sink — in one frameless window.
          </p>
        </div>

        <div className="hero__actions">
          <a className="btn btn--fill" href="#install">
            <ArrowDown aria-hidden="true" /> Download
          </a>
          <a className="btn" href={REPO_URL} target="_blank" rel="noreferrer noopener">
            <Github aria-hidden="true" /> GitHub
            <ArrowUpRight aria-hidden="true" />
          </a>
          <span className="label">AGPL-3.0-or-later</span>
        </div>

        <ul className="hero__specs">
          <li className="hero__spec">
            <b>05</b> effects
          </li>
          <li className="hero__spec">
            <b>31</b> EQ bands
          </li>
          <li className="hero__spec">
            <b>06</b> accents
          </li>
          <li className="hero__spec">
            <b>02</b> install formats
          </li>
        </ul>
      </div>

      <div className="hero__spectrum">
        <Spectrum bars={96} />
      </div>
    </section>
  );
}
