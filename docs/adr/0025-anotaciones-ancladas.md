# 0025. Anotaciones ancladas por cita y elemento

**Estado:** Aceptada

## Decisión

El texto se ancla por cita con contexto (prefijo y sufijo, como Hypothesis). El elemento, por id, selector CSS, etiqueta y texto. Si el texto desaparece, la anotación queda huérfana (`anchored: false`). Solo Rust escribe `.visor/anotaciones/`.
