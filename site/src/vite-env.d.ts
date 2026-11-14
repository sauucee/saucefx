/// <reference types="vite/client" />

// Debug/QA frame counter used by scripts/verify.mjs (global script file —
// interface merging here augments Window without making this a module).
interface Window {
  __signal?: { frames: number };
  /** Latest pointer position, 0..1 — read by the shader for parallax. */
  __signalMouse?: { x: number; y: number };
}

