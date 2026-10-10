# 0006. Un único WebContext

**Estado:** Aceptada

## Contexto

Cada página viva tiene su webview, y la ventana puede tener dos lados. Cada `WebContext` arranca su propio proceso de red de WebKit.

## Decisión

Todos los webviews comparten un único `WebContext`, que crea `ui/app.rs` y recibe cada página al construirse (`ui/page_view.rs`).

## Consecuencias

Con un contexto por lado había dos procesos de red escribiendo en las mismas bases SQLite («database is locked») y más memoria.
