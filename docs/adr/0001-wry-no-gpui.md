# 0001. wry en lugar de GPUI

**Estado:** Aceptada

## Decisión

wry sobre tao y GTK, no GPUI, para incrustar webviews de WebKitGTK. En Wayland, wry solo puede incrustar un webview en un contenedor GTK propio (`build_gtk`); bajo X11 admitiría una ventana ajena.
