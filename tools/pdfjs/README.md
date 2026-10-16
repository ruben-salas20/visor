# PDF.js para visor

«npm ci && npm run build» escribe `assets/pdfjs/`, que `build.rs` incrusta en el binario. La versión está fijada en `package.json` (`pdfjs-dist` 6.4.299, sin rangos) y en `package-lock.json`.

## Qué sale

- `pdf.min.mjs`: la API de PDF.js (`pdfjsLib`). Copiado tal cual, ya viene minificado.
- `pdf.worker.min.mjs`: el worker que hace el trabajo pesado. Se carga con `GlobalWorkerOptions.workerSrc`.
- `pdf_viewer.mjs`: el visor (páginas, búsqueda, enlaces). Minificado aquí. Lee `globalThis.pdfjsLib`, así que la página carga primero `pdf.min.mjs` y lo asigna.
- `pdf_viewer.css`: estilos del visor, con las imágenes que usa.
- `images/`: solo las imágenes que cita el CSS.
- `wasm/`: `openjpeg.wasm` (JPEG 2000), `jbig2.wasm` (JBIG2) y `qcms_bg.wasm` (perfiles de color), con sus licencias.

## Qué no entra

- **Scripting** (quickjs, `pdf.sandbox`): visor abre los PDF con `enableScripting: false`.
- **Fuentes y mapas de caracteres** (`cmaps/`, `standard_fonts/`, `iccs/`): sin ellos, un PDF con fuentes no incrustadas o texto CJK puede verse con letras sustituidas.
- **Fallbacks sin wasm** (`*_nowasm_fallback.js`), `.map`, `legacy/` y la build no minificada del worker.
- **Imágenes de web/images** que el CSS no usa.

## Rutas

La página del visor carga todo desde `__visor__/pdfjs/...` (el prefijo secreto de la bóveda). Nunca desde una CDN.
