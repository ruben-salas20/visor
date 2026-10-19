import path from 'node:path'
import { fileURLToPath } from 'node:url'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { defineConfig } from 'vite'

const aqui = path.dirname(fileURLToPath(import.meta.url))

// The Rust side serves the frame from web/react/ on a local HTTP server.
// `base: './'` keeps every asset path relative, so the folder works wherever it is served.
export default defineConfig({
  base: './',
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: { '@': path.resolve(aqui, 'src') },
  },
  build: {
    outDir: '../../web/react',
    emptyOutDir: true,
  },
})
