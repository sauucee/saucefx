import screenshot from '../assets/saucefx-main.png';
import { VERSION } from '../content/site';

/**
 * The real app capture, framed as a physical screen. Intrinsic dimensions are
 * declared so the hero reserves its space before the PNG decodes.
 */
export function AppScreenshot() {
  return (
    <div className="shot">
      <div className="shot__img">
        <img
          src={screenshot}
          width={1040}
          height={568}
          alt="The SauceFX desktop app: preset and output pickers, five effect faders and a 31-band equalizer with a live spectrum."
          fetchPriority="high"
          decoding="async"
          draggable={false}
        />
      </div>

      <span className="shot__tag">
        <span className="dot" />
        SauceFX {VERSION}
      </span>
    </div>
  );
}
