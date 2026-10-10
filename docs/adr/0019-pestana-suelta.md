# 0019. Un HTML suelto se abre como pestaña, sin crear bóveda

**Estado:** Aceptada · **Fecha:** 2026-10-07

## Contexto

Mostrar una muestra rápida no debería exigir una bóveda. Antes, la carpeta del archivo pasaba a ser bóveda para siempre: se vigilaba entera (podía ser `~` o `/tmp`) y cerraba las pestañas de la bóveda abierta.

## Decisión

Un `.html` fuera de toda bóveda conocida se abre como pestaña, sin crear bóveda.

- La pestaña guarda la ruta absoluta (las de bóveda nunca empiezan por `/`) y se sirve en `<base>/<ruta absoluta>`, con doble barra tras la clave. El servidor solo entrega archivos de las carpetas abiertas así.
- Recibe `bridge.js` (consola y `visor.send`), no anotaciones ni escritura: no puede escribir en una bóveda (frontera de confianza).
- Su carpeta se vigila sin subcarpetas.
- En frío, la CLI arranca la app con `VISOR_LOOSE`: sin la última bóveda y sin panel.
- Solo vale para HTML: un `.md` suelto abre su carpeta como bóveda, porque sus `[[enlaces]]` y el editor la necesitan.

## Cifras

2026-10-07, 5 rondas: arranque igual (~272 ms); memoria −3 MB frente a abrirlo como bóveda; binario +12 KB.
