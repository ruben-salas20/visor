# 0006. Un único WebContext

**Estado:** Aceptada

## Decisión

Todos los webviews de todas las ventanas comparten un `WebContext`, creado en `ui/visor.rs` y entregado a cada página al construirla (`ui/page_view.rs`). Cada `WebContext` arranca su propio proceso de red; con uno por lado, dos procesos escribían en las mismas bases SQLite («database is locked») y gastaban más memoria.
