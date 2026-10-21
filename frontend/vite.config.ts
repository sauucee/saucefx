import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// Relative base so the production build can be served from any origin,
// including a WebKit resource-provider scheme.
export default defineConfig({
  base: './',
  plugins: [react()],
  build: {
    target: 'es2022',
    outDir: 'dist',
    emptyOutDir: true,
    assetsInlineLimit: 8192,
  },
})
