# 0002. Un webview por cada página viva

**Estado:** Aceptada

## Decisión

Cada página viva tiene su webview en un `gtk::Paned`; no hay iframes para la vista dividida. WebKit limita a 30 fps los iframes de otro origen sin interacción. Cada webview extra cuesta un proceso de WebKit (ver 0011).
