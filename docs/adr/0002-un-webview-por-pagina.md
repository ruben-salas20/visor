# 0002. Un webview por cada página viva

**Estado:** Aceptada

## Decisión

Cada página viva tiene su webview en un `gtk::Paned`; no hay iframes para la vista dividida. WebKit limita a 30 fps los iframes de otro origen sin interacción. Cada webview extra cuesta un proceso de WebKit (ver 0011).

## Descartado: compartir proceso

Las páginas de la bóveda podrían compartir un proceso con `with_related_view`. Medido el 2026-10-10 (3 rondas, 3 pestañas de notas): pestaña nueva visible en 8–9 ms frente a 59–69 ms, y 252–257 MB frente a 345–348 MB. Se descartó por la frontera de confianza: WebKitGTK no cambia de proceso al navegar a una web ni separa los iframes de otro sitio, así que una web que comprometiera su proceso controlaría todas las páginas de la bóveda y sus claves IPC. Con un proceso por página solo alcanza la suya.
