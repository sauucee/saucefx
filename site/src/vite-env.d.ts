/// <reference types="vite/client" />

// Debug/QA frame counter used by scripts/verify.mjs (global script file —
// interface merging here augments Window without making this a module).
interface Window {
  __signal?: { frames: number };
}

