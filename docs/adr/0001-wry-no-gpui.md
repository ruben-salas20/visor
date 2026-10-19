# 0001. wry en lugar de GPUI

**Estado:** Aceptada; ampliada por el [0040](0040-interfaz-en-html-y-winit-en-windows.md) (winit en Windows, interfaz en HTML)

## Decisión

wry sobre tao y GTK, no GPUI, para incrustar webviews de WebKitGTK. En Wayland, wry solo puede incrustar un webview en un contenedor GTK propio (`build_gtk`); bajo X11 admitiría una ventana ajena.
