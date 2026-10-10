# 0028. Regenerar el paquete de CodeMirror

**Estado:** Aceptada

## Contexto

El paquete de CodeMirror vive en `assets/codemirror/codemirror.min.js` y se genera a partir de dependencias fijadas.

## Decisión

En `tools/codemirror/`, `npm ci && npm run build` escribe `assets/codemirror/codemirror.min.js`. Las versiones están fijadas en `package.json` y `package-lock.json`. `entry.js` exporta en `window.CM` lo que usa `src/editor.js`. Si este necesita algo más, se añade allí y se vuelve a generar.

## Consecuencias

`entry.js` exporta además `EditorSelection` y `styleTags`, que `src/editor.js` no usa. Se dejan porque quitarlos agranda el paquete comprimido en 49 bytes.

## Cifras

- Paquete comprimido con brotli: 89 406 bytes con `EditorSelection` y `styleTags`, y 89 455 bytes sin ellos (+49 bytes). Medido el 2026-10-09.
