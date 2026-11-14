/// <reference types="vite/client" />

declare global {
  interface Window {
    /** Debug/QA frame counter used by scripts/verify.mjs. */
    __signal?: { frames: number };
  }
}

