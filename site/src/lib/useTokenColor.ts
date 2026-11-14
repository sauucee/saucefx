import { useEffect, useState } from 'react';

/** Live value of a CSS custom property on <html>; re-reads when theme/accent flips. */
export function useTokenColor(name: string, fallback = '#050506'): string {
  const read = () =>
    getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;
  const [color, setColor] = useState(read);
  useEffect(() => {
    const obs = new MutationObserver(() => setColor(read()));
    obs.observe(document.documentElement, { attributeFilter: ['data-accent', 'data-theme'] });
    return () => obs.disconnect();
  }, [name]);
  return color;
}

/** Live --accent value. */
export function useAccentColor(): string {
  return useTokenColor('--accent', '#ff2e57');
}
