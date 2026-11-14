/**
 * User-facing motion switch (the nav's MOTION button), independent from the
 * OS-level prefers-reduced-motion. Stored on <html data-motion> so CSS can
 * react without JS, and mirrored to localStorage like the theme.
 */

const KEY = 'saucefx-site:motion';

export function motionOff(): boolean {
  return document.documentElement.dataset.motion === 'off';
}

export function storedMotionOff(): boolean {
  try {
    return localStorage.getItem(KEY) === 'off';
  } catch {
    return false;
  }
}

/** Apply before first paint (called from main.tsx alongside the theme IIFE). */
export function applyStoredMotion(): void {
  if (storedMotionOff()) document.documentElement.dataset.motion = 'off';
}

export function setMotionOff(off: boolean): void {
  if (off) document.documentElement.dataset.motion = 'off';
  else delete document.documentElement.dataset.motion;
  try {
    localStorage.setItem(KEY, off ? 'off' : 'on');
  } catch {
    /* ignore */
  }
}
