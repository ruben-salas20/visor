# 0001. wry en lugar de GPUI

**Estado:** Aceptada

## Contexto

La app debe incrustar webviews de WebKitGTK en una ventana de Wayland.

## Decisión

Se usa wry sobre tao y GTK, no GPUI.

## Consecuencias

wry solo incrusta un webview en una ventana ajena bajo X11. En Wayland exige su propio contenedor GTK (`build_gtk`).
