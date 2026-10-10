# 0004. Iconos dibujados con cairo, ninguno del tema

**Estado:** Aceptada

**Fecha:** 2026-10-09

## Contexto

Los iconos SVG del tema pasan por un cargador aislado (`glycin`) que retrasa el arranque en frío. GTK también dibuja con un SVG la flecha de las carpetas del árbol.

## Decisión

Ningún icono sale del tema (`icons.rs`). Cairo dibuja los de la barra y las pestañas. La flecha de las carpetas del árbol se oculta con `set_show_expanders(false)` y se pinta una propia en una celda.

## Consecuencias

Con un solo icono del tema se lanza glycin, así que basta uno para perder la mejora. Cualquier icono nuevo debe dibujarse con cairo.

## Cifras

- Con iconos del tema, glycin sumaba ~50 ms a 6 de cada 8 arranques en frío.
- Quitar todos los iconos del tema (2026-10-09, 5 rondas de arranque y 3 de memoria): arranque 269 → 219 ms; memoria −14,5 MB (glycin y sus dos `bwrap`); binario +8 KB.
