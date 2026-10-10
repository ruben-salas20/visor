# 0026. CodeMirror 6 ligero con vista previa en vivo

**Estado:** Aceptada

**Fecha:** 2026-10-07

## Contexto

Las notas necesitan un editor. La alternativa era un `<textarea>`.

## Decisión

CodeMirror 6 ligero con vista previa en vivo, como Obsidian, en lugar del `<textarea>`. La lectura no cambia: Ctrl+E alterna con el editor, y CodeMirror solo se descarga al entrar en edición.

## Consecuencias

Leer no cuesta nada extra. Editar cuesta memoria (ver «Cifras»).

## Cifras

- Prototipo: 90 KB con brotli. Con el editor abierto, +15 MB con una nota normal y +19 MB con una de 3 240 líneas; sin abrirlo, nada. Listo en 40–52 ms frente a 33–84 ms del textarea en la nota normal, y en 46–58 frente a 79–101 en la larga.
- Editor terminado (2026-10-07, release, `edicion.md`, 6 rondas): arranque y memoria al leer, iguales. Con el editor abierto, 334–335 MB frente a 309–310 MB del textarea (+25 MB). Binario +100 KB (paquete 89 KB y `editor.js`/`editor.css` 10 KB, con brotli).
