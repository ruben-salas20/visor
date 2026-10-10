# 0002. Un webview por cada página viva

**Estado:** Aceptada

## Contexto

Las páginas de la bóveda deben ir en una vista dividida. Una opción era mostrarlas en iframes.

## Decisión

Cada página viva tiene su propio webview, colocado en un `gtk::Paned`. No se usan iframes.

## Consecuencias

WebKit limita a 30 fps los iframes de otro origen sin interacción. Cada webview extra cuesta un proceso de WebKit (ver la decisión 0011).
