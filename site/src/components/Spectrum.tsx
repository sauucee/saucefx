import type { CSSProperties } from 'react';

/**
 * Decorative spectrum. Each bar is a CSS-animated scaleY, so the strip costs
 * one paint on mount and nothing per frame on the main thread.
 */
export function Spectrum({ bars = 88 }: { bars?: number }) {
  return (
    <div className="spectrum" aria-hidden="true">
      {Array.from({ length: bars }, (_, i) => {
        // Two incommensurate sines read as a mix rather than an obvious wave.
        const wave = Math.abs(
          Math.sin(i * 0.41 + 0.6) * 0.52 + Math.sin(i * 1.73 + 2.1) * 0.48,
        );
        const scale = 0.12 + wave * 0.82;
        const style = {
          '--s': scale.toFixed(3),
          '--d': `${(1.05 + (i % 9) * 0.18).toFixed(2)}s`,
          '--del': `${(((i * 37) % 100) / 100).toFixed(2)}s`,
        } as CSSProperties;

        return <span key={i} style={style} />;
      })}
    </div>
  );
}
