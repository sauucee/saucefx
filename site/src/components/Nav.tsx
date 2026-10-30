import { useEffect, useState } from 'react';
import { ArrowUpRight, Github, Menu, Moon, Sun, X } from 'lucide-react';
import logo from '../assets/saucefx-logo.svg';
import { ACCENTS, useTheme } from '../theme/ThemeProvider';
import { REPO_URL, VERSION } from '../content/site';

const LINKS = [
  { href: '#rack', label: 'Rack', id: 'rack' },
  { href: '#playground', label: 'EQ', id: 'playground' },
  { href: '#features', label: 'App', id: 'features' },
  { href: '#install', label: 'Install', id: 'install' },
];

function ThemeSwitch({ compact = false }: { compact?: boolean }) {
  const { theme, setTheme } = useTheme();
  return (
    <div className="seg" role="group" aria-label="Colour theme">
      <button
        type="button"
        className="seg__btn"
        aria-pressed={theme === 'dark'}
        aria-label="Dark theme"
        title="Dark"
        onClick={() => setTheme('dark')}
      >
        <Moon size={compact ? 14 : 13} strokeWidth={2} aria-hidden="true" />
      </button>
      <button
        type="button"
        className="seg__btn"
        aria-pressed={theme === 'light'}
        aria-label="Soft light theme"
        title="Soft light"
        onClick={() => setTheme('light')}
      >
        <Sun size={compact ? 14 : 13} strokeWidth={2} aria-hidden="true" />
      </button>
    </div>
  );
}

function AccentDots() {
  const { accent, setAccent } = useTheme();
  return (
    <div className="swatches" role="group" aria-label="Accent colour">
      {ACCENTS.map((item) => (
        <button
          key={item.id}
          type="button"
          className="swatch"
          style={{ ['--swatch' as string]: `var(--palette-${item.id})` }}
          aria-pressed={accent === item.id}
          aria-label={`${item.label} accent`}
          title={item.label}
          onClick={() => setAccent(item.id)}
        />
      ))}
    </div>
  );
}

export function Nav() {
  const [stuck, setStuck] = useState(false);
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState<string | null>(null);

  useEffect(() => {
    const onScroll = () => setStuck(window.scrollY > 16);
    onScroll();
    window.addEventListener('scroll', onScroll, { passive: true });
    return () => window.removeEventListener('scroll', onScroll);
  }, []);

  useEffect(() => {
    const elements = LINKS.map((link) => document.getElementById(link.id)).filter(
      (element): element is HTMLElement => Boolean(element),
    );
    if (!elements.length) return;

    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries
          .filter((entry) => entry.isIntersecting)
          .sort((a, b) => b.intersectionRatio - a.intersectionRatio)[0];
        if (visible) setActive(visible.target.id);
      },
      { rootMargin: '-40% 0px -50% 0px', threshold: [0.01, 0.2, 0.5] },
    );
    elements.forEach((element) => observer.observe(element));
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(false);
    };
    document.addEventListener('keydown', onKey);
    document.documentElement.style.overflow = 'hidden';
    return () => {
      document.removeEventListener('keydown', onKey);
      document.documentElement.style.overflow = '';
    };
  }, [open]);

  const close = () => setOpen(false);

  return (
    <>
      <header className="nav" data-stuck={stuck}>
        <div className="shell nav__in">
          <a href="#top" className="brand" onClick={close}>
            <img src={logo} alt="" width={26} height={26} />
            <span className="brand__name">SAUCEFX</span>
            <span className="brand__ver">v{VERSION}</span>
            <span className="sr-only">— back to top</span>
          </a>

          <nav className="nav__links" aria-label="Sections">
            {LINKS.map((link) => (
              <a
                key={link.href}
                className="nav__link"
                href={link.href}
                data-active={active === link.id}
              >
                {link.label}
              </a>
            ))}
          </nav>

          <div className="nav__tools">
            <div className="nav__accents">
              <AccentDots />
            </div>
            <ThemeSwitch />

            <a
              className="icon-btn"
              href={REPO_URL}
              target="_blank"
              rel="noreferrer noopener"
              aria-label="SauceFX on GitHub"
              title="GitHub"
            >
              <Github size={14} strokeWidth={2} aria-hidden="true" />
            </a>

            <a className="btn btn--sm btn--fill nav__dl" href="#install">
              Download
            </a>

            <button
              type="button"
              className="icon-btn nav__burger"
              aria-expanded={open}
              aria-controls="drawer"
              aria-label={open ? 'Close menu' : 'Open menu'}
              onClick={() => setOpen((value) => !value)}
            >
              {open ? (
                <X size={16} strokeWidth={2} aria-hidden="true" />
              ) : (
                <Menu size={16} strokeWidth={2} aria-hidden="true" />
              )}
            </button>
          </div>
        </div>
      </header>

      {open && (
        <div className="drawer" id="drawer">
          <nav className="drawer__links" aria-label="Mobile sections">
            {LINKS.map((link, i) => (
              <a key={link.href} href={link.href} className="drawer__link" onClick={close}>
                {link.label}
                <span>0{i + 1}</span>
              </a>
            ))}
            <a
              className="drawer__link"
              href={REPO_URL}
              target="_blank"
              rel="noreferrer noopener"
              onClick={close}
            >
              GitHub
              <span>
                <ArrowUpRight size={12} aria-hidden="true" />
              </span>
            </a>
          </nav>

          <div className="drawer__bottom">
            <div className="drawer__row">
              <span className="label">Theme</span>
              <ThemeSwitch compact />
            </div>
            <div className="drawer__row">
              <span className="label">Accent</span>
              <AccentDots />
            </div>
            <a className="btn btn--fill" href="#install" onClick={close}>
              Download for Linux
            </a>
            <p className="label">Sound EQ for Linux · AGPL-3.0-or-later</p>
          </div>
        </div>
      )}
    </>
  );
}
