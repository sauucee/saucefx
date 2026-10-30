import { EFFECTS } from '../content/site';

/** A four-point "signal" spark, drawn inline so the strip costs no request. */
function Spark() {
  return (
    <svg viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path
        d="M12 0c0 7 5 12 12 12-7 0-12 5-12 12 0-7-5-12-12-12 7 0 12-5 12-12Z"
        fill="currentColor"
      />
    </svg>
  );
}

function Group() {
  return (
    <div className="marquee__group">
      {[0, 1, 2].map((pass) =>
        EFFECTS.map((effect, i) => (
          <span
            className={`marquee__word${(i + pass) % 3 === 2 ? ' marquee__word--outline' : ''}`}
            key={`${pass}-${effect}`}
            aria-hidden={pass > 0 ? true : undefined}
          >
            {effect}
            <Spark />
          </span>
        )),
      )}
    </div>
  );
}

export function Marquee() {
  return (
    <div className="marquee-wrap">
      <div className="marquee" role="img" aria-label={`Five effects: ${EFFECTS.join(', ')}`}>
        <div className="marquee__track" aria-hidden="true">
          <Group />
          <Group />
        </div>
      </div>
    </div>
  );
}
