# 0004. Iconos dibujados con cairo, ninguno del tema

**Estado:** Sustituida por el [0040](0040-interfaz-en-html-y-winit-en-windows.md) (2026-10-25, en vigor con la 0.3.0) · **Fecha:** 2026-10-09

## Contexto

Los iconos SVG del tema pasan por un cargador aislado (`glycin`) que retrasa el arranque en frío. GTK también dibuja con un SVG la flecha de las carpetas del árbol.

## Decisión

Ningún icono sale del tema (`ui/icons.rs`): cairo dibuja los de la barra y las pestañas. La flecha del árbol se oculta con `set_show_expanders(false)` y se pinta una propia en una celda. Un solo icono del tema lanza glycin y pierde toda la mejora: todo icono nuevo se dibuja con cairo.

## Cifras

- Con iconos del tema, glycin sumaba ~50 ms a 6 de cada 8 arranques en frío.
- Quitarlos todos (2026-10-09, 5 rondas de arranque y 3 de memoria): arranque 269 → 219 ms; memoria −14,5 MB (glycin y sus dos `bwrap`); binario +8 KB.

## Sustitución (2026-10-25)

El marco en HTML del ADR 0040 reemplazó a la interfaz GTK: ya no hay panel, pestañas ni barra en GTK, y `ui/icons.rs` desapareció. Los iconos son SVG dentro del marco (`marco/`) y los dibuja WebKit, que no usa glycin.

Lo que sigue valiendo: la ventana GTK que queda en Linux no debe cargar ningún icono del tema, porque uno solo lanzaría glycin y devolvería los ~50 ms al arranque en frío. Las cifras de arriba son de la interfaz GTK y no se re-miden.
