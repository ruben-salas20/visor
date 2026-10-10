# 0025. Anotaciones ancladas por cita y elemento

**Estado:** Aceptada

## Contexto

Una página puede cambiar después de anotarla.

## Decisión

El texto se ancla por cita con contexto (prefijo y sufijo, como Hypothesis). El elemento se ancla por id, selector CSS, etiqueta y texto. Rust es el único que escribe `.visor/anotaciones/`.

## Consecuencias

Si el texto desaparece, la anotación queda huérfana (`anchored: false`).
