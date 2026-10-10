# 0028. Regenerar el paquete de CodeMirror

**Estado:** Aceptada

## Decisión

`npm ci && npm run build` en `tools/codemirror/` escribe `assets/codemirror/codemirror.min.js`. Las versiones están fijadas en `package.json` y `package-lock.json`. `entry.js` exporta en `window.CM` lo que usa `src/page/editor.js`; si este necesita algo más, se añade allí y se regenera.

`entry.js` exporta además `EditorSelection` y `styleTags`, que nadie usa: quitarlos agranda el paquete en 49 bytes.

## Cifras

Paquete con brotli (2026-10-09): 89 406 bytes con esos dos exports, 89 455 sin ellos.
