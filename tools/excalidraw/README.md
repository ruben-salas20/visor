# Excalidraw para visor

«npm ci && npm run build» escribe `assets/excalidraw/`, que `build.rs` incrusta en el binario. Las versiones están fijadas en `package.json` y `package-lock.json`.

## Qué sale

- `excalidraw.min.js`: IIFE con Excalidraw 0.18.1, React 18 y lz-string. Expone `window.Excalidraw` (`React`, `createRoot`, `Excalidraw`, `serializeAsJSON`, `restore`, `restoreElements`, `exportToSvg`, `convertToExcalidrawElements`, `MIME_TYPES`, `LZString`).
- `excalidraw.css`: la hoja de estilos de la librería. Cárgala antes de montar el editor.
- `fonts/`: las fuentes woff2 que pide la librería, con la misma estructura de rutas.

## EXCALIDRAW_ASSET_PATH

Antes de cargar `excalidraw.min.js`, define `window.EXCALIDRAW_ASSET_PATH` como URL absoluta con barra final, apuntando a esta carpeta. En visor: `config.assets + "excalidraw/"`, que lleva el prefijo secreto del servidor. Una ruta que empiece por `/` no sirve, porque no lleva ese prefijo.

## Qué no entra

- **Mermaid y KaTeX**: `build.mjs` cambia `@excalidraw/mermaid-to-excalidraw` por `stub-mermaid.js`, que lanza un error claro si se usa. El build falla si mermaid o katex entran en el paquete.
- **Xiaolai (CJK)**: 13 MB de subconjuntos. No se copia. Su fallback a esm.sh se reescribe para que apunte al mismo servidor (así no sale nada de la máquina), y el texto CJK se ve con la fuente del sistema.
- **Idiomas**: solo `en` y `es-ES`. El resto de traducciones son módulos vacíos.

## Límites conocidos

- El worker de subconjunto de fuentes no existe dentro de un único archivo. Las exportaciones con fuentes incrustadas lo hacen en el hilo principal y dejan un error en consola (`WorkerUrlNotDefinedError`).
