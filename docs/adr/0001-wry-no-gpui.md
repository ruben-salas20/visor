# 0001. wry en lugar de GPUI

**Estado:** Aceptada; ampliada por el [0040](0040-interfaz-en-html-y-winit-en-windows.md) (2026-10-25: en vigor con la 0.3.0)

## Decisión

wry sobre tao y GTK, no GPUI, para incrustar webviews de WebKitGTK. En Wayland, wry solo puede incrustar un webview en un contenedor GTK propio (`build_gtk`); bajo X11 admitiría una ventana ajena.

## Desde el ADR 0040

- **Linux:** sigue con wry sobre tao y GTK, pero GTK ya no dibuja la interfaz: solo da la ventana y el contenedor (`gtk::Fixed`) donde wry incrusta el marco en HTML y las vistas de las páginas (`ui/host/linux.rs`, `ui/shell/linux.rs`). La razón de esta decisión sigue en pie: en Wayland, wry solo incrusta en un contenedor GTK propio.
- **Windows:** wry sobre winit y WebView2, sin tao ni GTK (`ui/host/windows.rs`, `ui/shell/windows.rs`). tao colgó el bucle con varios webviews hijos (W2 en el 0040).
- **GPUI** sigue descartado: no puede incrustar webviews de WebKitGTK en Wayland (`build_as_child` de wry es solo X11). Se reconsideraría solo si wry gana subsuperficies Wayland.
