import { useEffect, useRef, useState } from 'react';
import { ArrowUpRight, Check, Copy, Plus } from 'lucide-react';
import {
  FLATPAK_COMMANDS,
  RELEASES_URL,
  REQUIREMENTS,
  REQUIREMENTS_INTRO,
  RPM_COMMANDS,
  VERSION,
} from '../content/site';

const TABS = [
  { id: 'flatpak', label: 'Flatpak', commands: FLATPAK_COMMANDS, note: 'Bundle ships the engine; the org.gnome.Platform//50 runtime is offered from Flathub if it is missing.' },
  { id: 'rpm', label: 'Fedora RPM', commands: RPM_COMMANDS, note: 'One command, latest release, straight from GitHub. DNF resolves every runtime dependency — no compiler, headers, Node.js or JUCE required.' },
] as const;

export function Install() {
  const [tab, setTab] = useState<(typeof TABS)[number]['id']>('flatpak');
  const [copied, setCopied] = useState<string | null>(null);
  const timer = useRef(0);

  useEffect(() => () => window.clearTimeout(timer.current), []);

  const copy = async (text: string, key: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(key);
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => setCopied(null), 1600);
    } catch {
      /* clipboard blocked — the command stays selectable */
    }
  };

  const active = TABS.find((item) => item.id === tab) ?? TABS[0];

  return (
    <section id="install" className="install sec" aria-labelledby="install-title">
      <div className="shell">
        <header className="sec__head">
          <span className="label">05 — Install</span>
          <span className="label">Linux · x86_64 · v{VERSION}</span>
        </header>

        <div className="install__grid">
          <div className="install__copy">
            <h2 id="install-title">
              One or two.
              <br />
              <em>That’s it.</em>
            </h2>
            <p>
              Ready-to-install packages are attached to tagged GitHub Releases. Pick the
              format for your distro, copy the block and run it.
            </p>

            <div className="install__actions">
              <a
                className="btn btn--fill"
                href={RELEASES_URL}
                target="_blank"
                rel="noreferrer noopener"
              >
                GitHub Releases
                <ArrowUpRight aria-hidden="true" />
              </a>
              <a className="btn" href="#playground">
                Try the EQ
              </a>
            </div>

            <details className="reqs">
              <summary>
                Build from source
                <Plus aria-hidden="true" strokeWidth={2} />
              </summary>
              <p className="reqs__intro">{REQUIREMENTS_INTRO}</p>
              <ul>
                {REQUIREMENTS.map((item, i) => (
                  <li key={item}>
                    <b>{`0${i + 1}`}</b>
                    <span>{item}</span>
                  </li>
                ))}
              </ul>
            </details>
          </div>

          <div>
            <div className="term">
              <div className="term__bar">
                <div className="term__tabs" role="tablist" aria-label="Install format">
                  {TABS.map((item) => (
                    <button
                      key={item.id}
                      type="button"
                      role="tab"
                      id={`tab-${item.id}`}
                      aria-selected={tab === item.id}
                      aria-controls="install-panel"
                      className="term__tab"
                      onClick={() => setTab(item.id)}
                    >
                      {item.label}
                    </button>
                  ))}
                </div>

                <span className="label">bash</span>
              </div>

              <div
                className="term__body"
                role="tabpanel"
                id="install-panel"
                aria-labelledby={`tab-${active.id}`}
              >
                {active.commands.map((command, i) => {
                  const key = `${active.id}-${i}`;
                  const isCopied = copied === key;
                  return (
                    <div className="cmd" key={key}>
                      <code>
                        <b>{i === 0 ? '$' : '›'}</b> {command}
                      </code>
                      <button
                        type="button"
                        className="cmd__copy"
                        data-copied={isCopied}
                        onClick={() => copy(command, key)}
                        aria-label={isCopied ? 'Copied' : `Copy command ${i + 1}`}
                        title={isCopied ? 'Copied' : 'Copy'}
                      >
                        {isCopied ? (
                          <Check aria-hidden="true" strokeWidth={2.4} />
                        ) : (
                          <Copy aria-hidden="true" strokeWidth={2} />
                        )}
                      </button>
                    </div>
                  );
                })}
              </div>

              <div className="term__foot">
                <span className="label">
                  <span className="dot" /> v{VERSION} · {active.label}
                </span>
                <a
                  className="label"
                  href={RELEASES_URL}
                  target="_blank"
                  rel="noreferrer noopener"
                >
                  All releases <ArrowUpRight size={11} aria-hidden="true" />
                </a>
              </div>
            </div>

            <p className="install__note">
              {active.note} SauceFX needs PipeWire and WirePlumber, plus{' '}
              <span className="mono">pactl</span> for the virtual sink.
            </p>
          </div>
        </div>
      </div>
    </section>
  );
}
