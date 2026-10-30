import { ArrowUpRight, Github } from 'lucide-react';
import logo from '../assets/saucefx-logo.svg';
import { LICENSE_URL, REPO_URL, RELEASES_URL, VERSION } from '../content/site';

export function Footer() {
  return (
    <footer className="foot">
      <div className="shell foot__cta">
        <div className="foot__eyebrow">
          <span className="label label--ink">
            <img src={logo} alt="" width={16} height={16} />
            SauceFX {VERSION}
          </span>
          <span className="label">Sound EQ for Linux</span>
        </div>

        <h2 className="foot__line">
          Stick to
          <br />
          the <span>sound</span>
          <i>.</i>
        </h2>

        <div className="foot__row">
          <p>
            One window, five faders and thirty-one bands — the FxSound DSP engine running
            natively on Linux.
          </p>

          <div className="foot__links">
            <a href={REPO_URL} target="_blank" rel="noreferrer noopener">
              <Github aria-hidden="true" /> GitHub
            </a>
            <a href={RELEASES_URL} target="_blank" rel="noreferrer noopener">
              Releases <ArrowUpRight aria-hidden="true" />
            </a>
            <a href="#install">Install</a>
            <a href={LICENSE_URL} target="_blank" rel="noreferrer noopener">
              AGPL-3.0
            </a>
          </div>
        </div>
      </div>

      <div className="shell foot__base">
        <span className="label">© 2026 SauceFX · AGPL-3.0-or-later</span>
        <span className="label">Not affiliated with FxSound or Power Technology.</span>
        <a className="label" href="#top">
          Back to top ↑
        </a>
      </div>
    </footer>
  );
}
