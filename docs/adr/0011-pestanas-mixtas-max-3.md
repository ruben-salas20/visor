# 0011. Pestañas mixtas con un máximo de 3 páginas vivas

**Estado:** Aceptada

**Fecha:** 2026-10-07

## Contexto

Cada página viva es un proceso de WebKit. Había tres opciones: todas vivas, solo la activa, o un máximo.

## Decisión

Hay pestañas mixtas, con un máximo de 3 páginas vivas en toda la ventana (`MAX_LIVE` en `app.rs`). Las páginas a la vista siempre cuentan. Al pasar de 3 se descarga la pestaña oculta que lleva más tiempo sin usarse, y se recarga al volver a ella.

## Consecuencias

El usuario eligió esto frente a «todas vivas» (la memoria crece sin límite) y «solo la activa» (recargar Three.js en cada cambio se nota). Una pestaña descargada empieza con la consola vacía.

## Cifras

- Prototipo con webviews extra (2026-10-07, 3 rondas, PSS de todo el árbol): cada página viva extra es un proceso de WebKit de 60–65 MB con una nota y ~90 MB con Three.js.
- Con una sola pestaña no cambia nada: 310–318 MB antes y después.
- Peor caso habitual (3 vivas): ~440 MB. Con 3 escenas de Three.js: ~530 MB.
