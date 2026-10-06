# Decisiones tomadas

Cada punto se midió o se acordó con el usuario. Para cambiar uno, trae datos nuevos y consúltalo antes.

## Arquitectura

- **wry, no GPUI.** wry solo incrusta un webview en una ventana ajena bajo X11; en Wayland exige su propio contenedor GTK (`build_gtk`).
- **Dos webviews** (panel y página) en un `gtk::Paned`, no un iframe: WebKit limita a 30 fps los iframes de otro origen sin interacción, y así la página tampoco alcanza al panel.
- **Un único `WebContext`** para los dos webviews. Con dos había dos procesos de red escribiendo en las mismas bases SQLite («database is locked») y más memoria.
- **La bóveda se sirve por HTTP en 127.0.0.1** (`http.rs`), no por un esquema propio: el reproductor de WebKitGTK y Media Source rechazan los esquemas propios. Puerto fijo guardado en la configuración, porque el origen (y con él `localStorage`) depende del puerto; todas las bóvedas comparten ese origen. Una clave secreta en la ruta, nueva en cada arranque, y la comprobación de `Host` frenan a otros programas locales y el DNS rebinding.
- **DMA-BUF activado.** Desactivarlo arranca ~0,5 s antes, pero Three.js cae de 60 a 26 fps. El usuario usa mucho Three.js.
- **Caché de WebKit en modo `WebBrowser` y sin límite.** `DocumentViewer` libera ~700 MB al salir de webs pesadas, pero las páginas con librerías de CDN se recargan más lentas y fallan sin conexión. El tamaño de la caché en disco no afecta al arranque.
- **Cerrar es cerrar.** El usuario rechazó ocultar la ventana al cerrar (costaría ~240 MB permanentes).

## Comportamiento

- **Vigilancia de archivos**: `notify` también avisa al *abrir* un archivo; esos eventos se descartan o servir una página la recarga sin fin. Las escrituras propias se reconocen por el hash del contenido escrito, no por una ventana de tiempo, para no tapar cambios de un agente.
- **Enlaces**: http/https navegan dentro de visor (también `target="_blank"`), `mailto:` va a `xdg-open`, el resto se ignora.
- **Pantalla completa** oculta panel y barra, y restaura el panel solo si estaba visible.
- **Tema**: se lee de `~/.local/state/omarchy/current/theme/colors.toml` y se vigila esa carpeta. Las páginas reciben `--visor-*` como hoja de estilo de usuario, que su propio CSS siempre puede sobrescribir. `fc-match` (fuente monoespaciada) corre aparte porque tarda ~20 ms. La transparencia de la ventana la decide el usuario en Hyprland.
- **Anotaciones**: el texto se ancla por cita con contexto (prefijo y sufijo, como Hypothesis) y el elemento por id, selector CSS, etiqueta y texto. Rust es el único que escribe `.visor/anotaciones/`.

## Compilación y peso

- `panic = "abort"` y `opt-level = 3`. `"s"` ahorraba 380 KB pero renderizaba Markdown un 8–15 % más lento; `"z"`, un 50 %.
- Las librerías de `assets/` van comprimidas con brotli: WebKit solo anuncia gzip por HTTP, pero descodifica brotli. `build-override` optimiza el script de compilación (sin optimizar, brotli tardaba 39 s).
- Mermaid completo (1,15 MB comprimido): la versión *tiny* ahorra 0,6 MB pero pierde los mapas mentales. Pendiente de que el usuario decida.

## Requisitos del sistema

Vídeo web y MSE necesitan `gst-libav` (AAC, H.264) y `gst-plugins-bad` (`opusparse`). Recomendados: `gst-plugin-va` (decodificación por GPU) y `gst-plugin-dav1d`.
