# 0028. Regenerar el paquete de CodeMirror

**Estado:** Aceptada

## Decisión

`npm ci && npm run build` en `tools/codemirror/` escribe `assets/codemirror/codemirror.min.js`. Las versiones están fijadas en `package.json` y `package-lock.json`. `entry.js` exporta en `window.CM` lo que usa `src/page/editor.js`; si este necesita algo más, se añade allí y se regenera.

El build es `build.mjs`, que llama a esbuild con un plugin que parchea `@codemirror/view` al empaquetar: el buffer de widget pasa de `<img>` a `<span style="display: inline-block">` (ADR 0038). El plugin falla si el fuente fijado cambia de forma, para que el parche no pase inadvertido al actualizar.

`entry.js` exporta además `EditorSelection` y `styleTags`, que nadie usa: quitarlos agranda el paquete en 49 bytes.

## Cifras

Paquete con brotli (2026-10-09): 89 406 bytes con esos dos exports, 89 455 sin ellos.

Con `@codemirror/search` y la búsqueda sin tildes (#38, 2026-10-18): 96 032 bytes, +6 626.

Con el buffer de widget parcheado (ADR 0038, 2026-10-18): 89 382 bytes antes de la búsqueda; 96 073 con ella, +41 sobre los 96 032 sin el parche.
