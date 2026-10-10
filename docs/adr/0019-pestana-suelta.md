# 0019. Un HTML suelto se abre como pestaña, sin crear bóveda

**Estado:** Aceptada

**Fecha:** 2026-10-07

## Contexto

Mostrar una muestra rápida no debería exigir una bóveda.

## Decisión

Un `.html` fuera de toda bóveda conocida se abre como pestaña, sin crear bóveda. La pestaña guarda la ruta absoluta (las de bóveda nunca empiezan por `/`) y se sirve en `<base>/<ruta absoluta>`, es decir, con doble barra tras la clave. El servidor solo entrega archivos de las carpetas abiertas así. Reciben `bridge.js` (consola y `visor.send`) pero no anotaciones ni escritura. Su carpeta se vigila sin subcarpetas. En frío, la CLI arranca la app con `VISOR_LOOSE`: sin la última bóveda y sin panel. Solo vale para HTML: una nota `.md` suelta sigue abriendo su carpeta como bóveda, porque sus `[[enlaces]]` y el editor necesitan una bóveda.

## Consecuencias

Los HTML sueltos no pueden escribir en una bóveda (frontera de confianza).

## Antes

La carpeta del archivo pasaba a ser bóveda para siempre. Se vigilaba entera (podía ser `~` o `/tmp`) y cerraba las pestañas de la bóveda abierta. Se cambió por esos tres problemas.

## Cifras

- Medido el 2026-10-07, 5 rondas: arranque igual (~272 ms); memoria −3 MB frente a abrirlo como bóveda; binario +12 KB.
