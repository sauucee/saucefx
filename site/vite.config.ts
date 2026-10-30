import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

// `base: './'` keeps every asset relative so the build works from a GitHub
// Pages project subpath (/saucefx-site/) and from a Vercel root alike.
export default defineConfig({
  base: './',
  plugins: [react(), tailwindcss()],
  build: {
    target: 'es2020',
    cssMinify: 'lightningcss',
    assetsInlineLimit: 2048,
  },
});
