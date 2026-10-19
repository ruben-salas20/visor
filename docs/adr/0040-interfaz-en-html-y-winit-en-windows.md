# 0040. Interfaz en HTML, y winit en Windows

**Estado:** Aceptada (2026-10-19, issue #35)

## Contexto

visor solo compila en Linux. Para Windows hay que rehacer la interfaz GTK (panel, pestañas, barra) y cambiar la ventana.

## Decisión

- La interfaz pasa a un marco en HTML (React + shadcn/ui, cargando solo lo que se ve) en su propio webview, en Linux y en Windows. Rust conserva la lógica (pestañas, bóveda) y la frontera de confianza; el marco dibuja y pide.
- Cada página sigue en su webview hijo, aislada del marco: Rust decide qué vista habla.
- Ventana: tao + GTK en Linux (en Wayland, wry solo incrusta en un contenedor GTK, ADR 0001); winit en Windows (tao colgó el bucle con varios webviews hijos).
- Iconos: SVG dentro del marco; WebKit los dibuja sin glycin, así que el ADR 0004 deja de aplicar cuando el marco sustituya a GTK.
- Descartado: egui y slint (en Wayland no admiten webviews de wry sin GTK) y GPUI.

## Cifras

- Windows, W2: tao se colgó 2 de 10 veces; winit, 0 de 10.
- U1 en Windows: 0 cuelgues en 15; 8 de 8 órdenes hostiles rechazadas; unos 25 MB por página extra.
- U1 frente a visor (Linux, PSS): visor GTK 213–215 MB; marco React 309–324 MB (+97 a +110). Coste aceptado por el usuario para ir a Windows; se intentará recuperar cargando solo lo visible.

Detalle: `docs/plan-multiplataforma.md`.
