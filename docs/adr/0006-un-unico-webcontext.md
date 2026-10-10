# 0006. Un único WebContext

**Estado:** Aceptada

## Contexto

La ventana puede tener dos lados, cada uno con webviews.

## Decisión

Todos los webviews comparten un único `WebContext`.

## Consecuencias

Con dos había dos procesos de red escribiendo en las mismas bases SQLite («database is locked») y más memoria.
