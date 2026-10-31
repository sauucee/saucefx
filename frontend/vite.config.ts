import { readFileSync } from 'node:fs'
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

const packageJson = JSON.parse(
  readFileSync(new URL('./package.json', import.meta.url), 'utf8'),
) as { version: string }

// Relative base so the production build can be served from any origin,
// including a WebKit resource-provider scheme.
export default defineConfig({
  base: './',
  plugins: [react()],
  define: {
    __SAUCEFX_VERSION__: JSON.stringify(packageJson.version),
  },
  build: {
    target: 'es2022',
    outDir: 'dist',
    emptyOutDir: true,
    assetsInlineLimit: 8192,
  },
})
