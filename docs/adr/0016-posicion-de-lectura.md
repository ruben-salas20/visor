# 0016. Posición de lectura al ocultar una pestaña

**Estado:** Aceptada

## Contexto

Una pestaña descargada se recarga y perdería su posición.

## Decisión

Al ocultar una pestaña se le pide su posición. En las notas es la línea del Markdown y la altura, como al alternar con el editor. De paso se le pide que guarde el texto pendiente del editor. Si la pestaña se descarga, se recoloca al recargarla.

## Consecuencias

Rust solo guarda los números, porque la página puede ser una web.
