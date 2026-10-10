# 0011. Pestañas mixtas con un máximo de 3 páginas vivas

**Estado:** Aceptada · **Fecha:** 2026-10-07

## Contexto

Cada página viva es un proceso de WebKit. Opciones: todas vivas (la memoria crece sin límite), solo la activa (recargar Three.js en cada cambio se nota) o un máximo. El usuario eligió el máximo.

## Decisión

Máximo de 3 páginas vivas en toda la ventana (`MAX_LIVE` en `ui/tabs.rs`). Las páginas a la vista siempre cuentan. Al pasar de 3 se descarga la pestaña oculta que lleva más tiempo sin usarse y se recarga al volver a ella, con la consola vacía.

## Cifras

Prototipo con webviews extra (2026-10-07, 3 rondas, PSS de todo el árbol):

- Cada página viva extra: 60–65 MB con una nota, ~90 MB con Three.js.
- Con una sola pestaña no cambia nada: 310–318 MB antes y después.
- Peor caso habitual (3 vivas): ~440 MB. Con 3 escenas de Three.js: ~530 MB.
