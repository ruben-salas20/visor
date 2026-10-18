# 0028. Regenerar el paquete de CodeMirror

**Estado:** Aceptada

## Decisión

`npm ci && npm run build` en `tools/codemirror/` escribe `assets/codemirror/codemirror.min.js`. Las versiones están fijadas en `package.json` y `package-lock.json`. `entry.js` exporta en `window.CM` lo que usa `src/page/editor.js`; si este necesita algo más, se añade allí y se regenera.

El build es `build.mjs`, que llama a esbuild con un plugin que parchea `@codemirror/view` al empaquetar: el buffer de widget pasa de `<img>` a `<span style="display: inline-block">` (ADR 0038). El plugin falla si el fuente fijado cambia de forma, para que el parche no pase inadvertido al actualizar.

`entry.js` exporta además `EditorSelection` y `styleTags`, que nadie usa: quitarlos agranda el paquete en 49 bytes.

## Cifras

Paquete con brotli: 89 406 bytes el 2026-10-09 con esos dos exports (89 455 sin ellos); 89 382 el 2026-10-18 con el buffer parcheado (ADR 0038).
