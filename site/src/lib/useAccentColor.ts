import { useEffect, useState } from 'react';

/** Live --accent value; re-reads when <html data-accent|data-theme> flips. */
export function useAccentColor(): string {
  const read = () =>
    getComputedStyle(document.documentElement).getPropertyValue('--accent').trim() ||
    '#ff2e57';
  const [color, setColor] = useState(read);
  useEffect(() => {
    const obs = new MutationObserver(() => setColor(read()));
    obs.observe(document.documentElement, { attributeFilter: ['data-accent', 'data-theme'] });
    return () => obs.disconnect();
  }, []);
  return color;
}
