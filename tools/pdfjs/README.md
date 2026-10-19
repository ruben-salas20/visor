# PDF.js para visor

«npm ci && npm run build» escribe `assets/pdfjs/`, que `build.rs` incrusta en el binario. La versión está fijada en dos sitios, que el script comprueba que coinciden:

- `package.json` y `package-lock.json`: `pdfjs-dist` 6.4.299, sin rangos. Da la biblioteca y el worker ya minificados.
- `build.mjs`: el zip oficial de la versión (`pdfjs-6.4.299-dist.zip`), con su SHA-256. Es el único que trae el visor completo (`web/viewer.*`). Se descarga una vez en `cache/`, que git no sigue.

## Qué sale

- `pdf.min.mjs`: la API de PDF.js (`pdfjsLib`). Copiado tal cual.
- `pdf.worker.min.mjs`: el worker que hace el trabajo pesado.
- `viewer.mjs`: el visor completo de Firefox, minificado aquí. Lee `globalThis.pdfjsLib`, así que la página (`src/page/pdf.js`) carga primero `pdf.min.mjs` y lo asigna.
- `viewer.css`: sus estilos, minificados.
- `viewer-body.html`: el cuerpo de `web/viewer.html`, sin comentarios. La página de visor escribe la cabecera y mete este HTML en el cuerpo antes de arrancar el visor.
- `images/`: las imágenes que citan el CSS o el HTML.
- `locale/`: los textos del visor en `en-US` y `es-ES`, con un `locale.json` que solo lista esos dos.
- `wasm/`: `openjpeg.wasm` (JPEG 2000), `jbig2.wasm` (JBIG2) y `qcms_bg.wasm` (perfiles de color), con sus licencias.

## Qué no entra

- **Scripting** (quickjs, `pdf.sandbox`): visor abre los PDF con `enableScripting: false`.
- **Fuentes y mapas de caracteres** (`cmaps/`, `standard_fonts/`, `iccs/`): sin ellos, un PDF con fuentes no incrustadas o texto CJK puede verse con letras sustituidas. El visor no los pide: `pdf.js` le pasa `null` en esas rutas.
- **Los demás idiomas** de `locale/` (112 más) y `debugger.mjs`.
- **Fallbacks sin wasm** (`*_nowasm_fallback.js`), `.map`, `legacy/` y la build no minificada del worker.
- **Imágenes de web/images** que no citan ni el CSS ni el HTML.

## Rutas

La página del visor carga todo desde `__visor__/pdfjs/...` (el prefijo secreto de la bóveda). Nunca desde una CDN.
