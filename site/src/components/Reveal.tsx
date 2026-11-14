import { useRef, type ReactNode } from 'react';
import gsap from 'gsap';
import { SplitText } from 'gsap/SplitText';
import { ScrollTrigger } from 'gsap/ScrollTrigger';
import { useGSAP } from '@gsap/react';
import { prefersReducedMotion } from '../lib/usePrefersReducedMotion';
import { motionOff } from '../lib/motionPreference';

gsap.registerPlugin(SplitText, ScrollTrigger, useGSAP);

type RevealProps = {
  children: ReactNode;
  as?: 'h1' | 'h2' | 'h3' | 'p' | 'span' | 'div';
  className?: string;
  id?: string;
  /** 'chars' for display type, 'words' for running copy. */
  mode?: 'chars' | 'words';
  /** Start immediately after mount instead of waiting for scroll. */
  immediate?: boolean;
};

/**
 * SplitText kinetic reveal. Splits after fonts settle so metrics are stable;
 * reduced motion or data-motion="off" visitors get plain text, never split.
 */
export function Reveal({
  children,
  as = 'span',
  className,
  id,
  mode = 'words',
  immediate,
}: RevealProps) {
  const ref = useRef<HTMLElement>(null);
  const Tag = as;

  useGSAP(
    () => {
      const el = ref.current;
      if (!el || prefersReducedMotion() || motionOff()) return;

      let split: SplitText | undefined;
      let timeout = 0;

      const run = () => {
        split = new SplitText(el, {
          type: mode === 'chars' ? 'words,chars' : 'words',
          charsClass: 'rv-char',
          wordsClass: 'rv-word',
        });
        const targets = mode === 'chars' ? split.chars : split.words;
        if (!targets) return;
        gsap.set(targets, { yPercent: 110, opacity: 0 });
        gsap.to(targets, {
          yPercent: 0,
          opacity: 1,
          duration: mode === 'chars' ? 1.1 : 0.8,
          ease: 'expo.out',
          stagger: mode === 'chars' ? 0.028 : 0.05,
          ...(immediate
            ? { delay: 0.05 }
            : { scrollTrigger: { trigger: el, start: 'top 88%', once: true } }),
        });
      };

      // Split only once metrics are final — splitting a loading font mis-measures.
      void document.fonts?.ready.then(() => {
        if (motionOff()) return;
        timeout = window.setTimeout(run, immediate ? 0 : 0);
      });

      return () => {
        window.clearTimeout(timeout);
        split?.revert();
      };
    },
    { scope: ref },
  );

  return (
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    <Tag ref={ref as any} className={className} id={id}>
      {children}
    </Tag>
  );
}
