import path from 'node:path'
import { fileURLToPath } from 'node:url'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { defineConfig } from 'vite'

const here = path.dirname(fileURLToPath(import.meta.url))

// The build goes into assets/marco/, which build.rs packs into the binary
// with Brotli: `cargo build` needs no Node. Names carry no hash, so a rebuild
// changes only the files that changed; the frame's URL has a new secret on
// every start, so no stale copy is read from the cache.
export default defineConfig({
  base: './',
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: { '@': path.resolve(here, 'src') },
  },
  build: {
    outDir: path.resolve(here, '../assets/marco'),
    emptyOutDir: true,
    assetsDir: '.',
    modulePreload: { polyfill: false },
    rollupOptions: {
      output: {
        entryFileNames: '[name].js',
        chunkFileNames: '[name].js',
        assetFileNames: '[name][extname]',
      },
    },
  },
})
