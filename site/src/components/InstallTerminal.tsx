import { useEffect, useRef, useState } from 'react';
import { Check, Copy } from 'lucide-react';
import { FLATPAK_COMMANDS, RPM_COMMANDS } from '../content/site';
import { prefersReducedMotion } from '../lib/usePrefersReducedMotion';

type Dist = 'flatpak' | 'rpm';

const SETS: Record<Dist, { label: string; lines: string[] }> = {
  flatpak: { label: 'Flatpak', lines: FLATPAK_COMMANDS },
  rpm: { label: 'Fedora RPM', lines: RPM_COMMANDS },
};

/**
 * Chapter 6 — install as a terminal, not a card grid. The command types
 * itself once, then behaves like a real prompt: tabs switch the distro,
 * every line is copyable.
 */
export function InstallTerminal() {
  const [dist, setDist] = useState<Dist>('flatpak');
  const [typed, setTyped] = useState(0);
  const [copied, setCopied] = useState<number | null>(null);
  const body = useRef<HTMLDivElement>(null);

  const lines = SETS[dist].lines;

  // type the first command on entry — once, respecting reduced motion
  useEffect(() => {
    const el = body.current;
    if (!el) return;
    if (prefersReducedMotion()) {
      setTyped(lines[0].length);
      return;
    }
    let raf = 0;
    let started = false;
    const io = new IntersectionObserver(
      (entries) => {
        if (!entries[0]?.isIntersecting || started) return;
        started = true;
        io.disconnect();
        const start = performance.now();
        const perChar = 34;
        const tick = (now: number) => {
          setTyped(Math.min(lines[0].length, Math.floor((now - start) / perChar)));
          if ((now - start) / perChar < lines[0].length) raf = requestAnimationFrame(tick);
        };
        raf = requestAnimationFrame(tick);
      },
      { threshold: 0.4 },
    );
    io.observe(el);
    return () => {
      io.disconnect();
      cancelAnimationFrame(raf);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const copy = async (text: string, index: number) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(index);
      window.setTimeout(() => setCopied(null), 1400);
    } catch {
      /* clipboard unavailable — no-op */
    }
  };

  return (
    <section id="install" className="install" aria-labelledby="install-title">
      <div className="shell">
        <header className="install__head">
          <span className="label">Chapter 04 — install</span>
          <h2 id="install-title" className="install__title">
            Two commands.
            <br />
            <em>That's it.</em>
          </h2>
        </header>

        <div className="term" ref={body}>
          <div className="term__bar">
            <span className="term__dot" />
            <span className="term__dot" />
            <span className="term__dot" />
            <span className="label term__name">saucefx — sh</span>
            <div className="term__tabs" role="tablist" aria-label="Install method">
              {(Object.keys(SETS) as Dist[]).map((key) => (
                <button
                  key={key}
                  type="button"
                  role="tab"
                  aria-selected={dist === key}
                  className="term__tab label"
                  onClick={() => setDist(key)}
                >
                  {SETS[key].label}
                </button>
              ))}
            </div>
          </div>

          <div className="term__body" role="tabpanel">
            {lines.map((line, i) => (
              <div className="term__line" key={line}>
                <span className="term__prompt">$</span>
                <code className="term__cmd">
                  {i === 0 ? line.slice(0, typed) : line}
                  {i === 0 && typed < line.length && (
                    <span className="term__caret" aria-hidden="true" />
                  )}
                </code>
                <button
                  type="button"
                  className="term__copy label"
                  onClick={() => copy(line, i)}
                  aria-label={copied === i ? 'Copied' : `Copy command ${i + 1}`}
                >
                  {copied === i ? (
                    <>
                      <Check size={12} aria-hidden="true" /> Copied
                    </>
                  ) : (
                    <>
                      <Copy size={12} aria-hidden="true" /> Copy
                    </>
                  )}
                </button>
              </div>
            ))}
            <p className="term__out" aria-hidden="true">
              ✔ SauceFX {SETS[dist].label === 'Flatpak' ? 'installed — flatpak run to start' : 'installed — dnf handled the deps'}
            </p>
          </div>
        </div>
      </div>
    </section>
  );
}
