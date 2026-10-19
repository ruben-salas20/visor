# U2: webview de wry dentro de eframe o slint en Wayland

Experimento del issue 35 (opción B: marco en egui o slint, páginas con wry). Pregunta: ¿puede un webview de wry 0.57 vivir dentro de una ventana de egui (eframe) o de slint, en Linux con Wayland nativo?

## Conclusión

**B se descarta en Wayland porque wry 0.57 rechaza el manejador de ventana de Wayland en `build_as_child`.** Con `build_gtk` sí aparece un webview, pero dentro de una ventana GTK aparte, no dentro de la ventana del toolkit. Eso no cumple lo que pide B.

## Qué se probó

Versiones: wry 0.57.0, gtk 0.18.2 (GTK 3), eframe 0.33.3, slint 1.18.1 (backend winit 0.30.13), raw-window-handle 0.6.2. WebKitGTK 2.54.1 y GTK 3.24.52 del sistema. Sesión: sway sin pantalla con Wayland (`WAYLAND_DISPLAY`, `DISPLAY` sin definir).

| Intento | Resultado | Error exacto o evidencia |
|---|---|---|
| eframe, `build_as_child(&frame)` | Falla | El manejador que da eframe es `Wayland` (lo imprime el programa). `Err: UnsupportedWindowHandle / the window handle kind is not supported` |
| eframe, `gtk::init` + `build_gtk` sobre `gtk::Fixed` en una ventana GTK propia | Funciona, pero en otra ventana | Captura `capturas/b-gtk-ventana-separada.png`: la página verde se ve en la ventana «U2 GTK separada», al lado de la ventana «U2 eframe». Hay que bombear el bucle de GTK a mano en cada `update` |
| slint (winit), `build_as_child(window)` con el manejador de winit | Falla | El manejador es `Wayland`. `Err: UnsupportedWindowHandle / the window handle kind is not supported` |

No probado: `build_as_child` con un manejador X11 (por XWayland). Haría falta ejecutar el toolkit en X11, y eso no es Wayland nativo. No probado tampoco incrustar widgets GTK dentro de una superficie de winit (subsuperficies de Wayland): GTK 3 no ofrece ese camino.

## Por qué falla: el código de wry 0.57.0

Ruta: `~/.cargo/registry/src/*/wry-0.57.0/`

- `src/lib.rs:1556-1557`: la documentación de `build_as_child` dice «Only X11 is supported. This method won't work on Wayland.»
- `src/lib.rs:1571-1576`: `build_as_child` llama a `InnerWebView::new_as_child`, sin más comprobaciones.
- `src/webkitgtk/mod.rs:125-141`: `new_as_child` solo llama a `new_x11` si está activa la feature `x11`. Si no, devuelve `UnsupportedWindowHandle`.
- `src/webkitgtk/mod.rs:150-153`: `new_x11` acepta solo `RawWindowHandle::Xlib`. Cualquier otro manejador, como Wayland, devuelve `UnsupportedWindowHandle`.
- `src/lib.rs:54` y `src/lib.rs:17`: la propia documentación de wry recomienda `build_gtk` para soportar Wayland.
- `src/lib.rs:2039-2050`: `build_gtk` coloca el webview en un contenedor GTK (`gtk::Fixed`, `gtk::Box`…). Requiere que `gtk::init` se haya llamado antes.

El fallo es de wry, no de eframe ni de slint: ambos entregan un manejador Wayland correcto.

## Archivos

- `Cargo.toml`: proyecto independiente con `[workspace]` vacío (mismo patrón que `spikes/w2-webviews`).
- `src/main.rs`: eframe con panel lateral. Modo `U2_MODO=child` (intento a) o `U2_MODO=gtk` (intento b).
- `src/bin/slint_child.rs`: slint con la ventana de winit y el intento a.
- `capturas/b-gtk-ventana-separada.png`: captura del intento b.
