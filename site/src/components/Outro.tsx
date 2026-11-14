import { ArrowUpRight, Github } from 'lucide-react';
import { LICENSE_URL, REPO_URL, RELEASES_URL, VERSION } from '../content/site';

/**
 * Chapter 7 — end of signal. Credits, not a card footer.
 */
export function Outro() {
  return (
    <footer id="outro" className="outro">
      <div className="shell">
        <p className="outro__mark label" aria-hidden="true">
          — end of signal —
        </p>
        <p className="outro__line">
          SauceFX {VERSION}. The FxSound DSP engine, native on Linux.
        </p>
        <p className="outro__fine">
          Independent from FxSound — not endorsed by them. DSP engine &amp; legacy
          artwork by FxSound, AGPL-3.0-or-later.
        </p>
        <nav className="outro__links" aria-label="Footer">
          <a href={REPO_URL} target="_blank" rel="noreferrer noopener">
            <Github size={13} aria-hidden="true" /> Source <ArrowUpRight size={11} aria-hidden="true" />
          </a>
          <a href={RELEASES_URL} target="_blank" rel="noreferrer noopener">
            Releases <ArrowUpRight size={11} aria-hidden="true" />
          </a>
          <a href={LICENSE_URL} target="_blank" rel="noreferrer noopener">
            Licence <ArrowUpRight size={11} aria-hidden="true" />
          </a>
        </nav>
      </div>
    </footer>
  );
}
