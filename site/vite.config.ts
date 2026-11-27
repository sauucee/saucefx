import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

// `base: './'` keeps every asset relative so the build works from a GitHub
// Pages project subpath (/saucefx-site/) and from a Vercel root alike.
export default defineConfig({
  base: './',
  plugins: [react(), tailwindcss()],
  // `src/content/site.ts` inlines the root Cargo.toml with `?raw` so the
  // release version is derived at build time. Allow only this site directory
  // and that single file — not the whole parent folder.
  server: {
    fs: { allow: ['.', '../Cargo.toml'] },
  },
  build: {
    target: 'es2020',
    cssMinify: 'lightningcss',
    assetsInlineLimit: 2048,
  },
});
