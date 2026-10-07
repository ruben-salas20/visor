# Decisiones tomadas

Cada punto se midió o se acordó con el usuario. Para cambiar uno, trae datos nuevos y consúltalo antes.

## Arquitectura

- **wry, no GPUI.** wry solo incrusta un webview en una ventana ajena bajo X11; en Wayland exige su propio contenedor GTK (`build_gtk`).
- **Un webview para el panel y otro por cada página viva**, en `gtk::Paned`, no iframes: WebKit limita a 30 fps los iframes de otro origen sin interacción, y así la página tampoco alcanza al panel.
- **Un único `WebContext`** para los dos webviews. Con dos había dos procesos de red escribiendo en las mismas bases SQLite («database is locked») y más memoria.
- **La bóveda se sirve por HTTP en 127.0.0.1** (`http.rs`), no por un esquema propio: el reproductor de WebKitGTK y Media Source rechazan los esquemas propios. Puerto fijo guardado en la configuración, porque el origen (y con él `localStorage`) depende del puerto; todas las bóvedas comparten ese origen. Una clave secreta en la ruta, nueva en cada arranque, y la comprobación de `Host` frenan a otros programas locales y el DNS rebinding.
- **DMA-BUF activado.** Desactivarlo arranca ~0,5 s antes, pero Three.js cae de 60 a 26 fps. El usuario usa mucho Three.js.
- **Caché de WebKit en modo `WebBrowser` y sin límite.** `DocumentViewer` libera ~700 MB al salir de webs pesadas, pero las páginas con librerías de CDN se recargan más lentas y fallan sin conexión. El tamaño de la caché en disco no afecta al arranque.
- **Cerrar es cerrar.** El usuario rechazó ocultar la ventana al cerrar (costaría ~240 MB permanentes).

## Pestañas y vista dividida

- **Pestañas mixtas, con un máximo de 3 páginas vivas** en toda la ventana (`MAX_LIVE` en `app.rs`). Las páginas a la vista siempre cuentan; al pasar de 3 se descarga la pestaña oculta que lleva más tiempo sin usarse, y se recarga al volver a ella. El usuario eligió esto frente a «todas vivas» (la memoria crece sin límite) y «solo la activa» (recargar Three.js en cada cambio se nota).
- **Coste medido** (2026-10-07, prototipo con webviews extra, 3 rondas, PSS de todo el árbol): cada página viva extra es un proceso de WebKit de **60–65 MB** con una nota y **~90 MB** con Three.js. Con una sola pestaña no cambia nada: 310–318 MB antes y después. El peor caso habitual (3 vivas) ronda 440 MB; con 3 escenas de Three.js, ~530 MB.
- **Cada lado tiene sus pestañas**, como Obsidian. Las pestañas, la barra y los botones de anotaciones son GTK nativo (`panes.rs`), igual que la barra. La CLI y los atajos actúan sobre el lado con el foco.
- **Unir los lados** pasa las pestañas de la derecha a la izquierda; los webviews cambian de contenedor sin recargarse. El divisor se guarda como proporción del ancho (`split_ratio`).
- **Posición de lectura**: al ocultar una pestaña se le pide su posición (en las notas, línea del Markdown y altura, como al alternar con el editor) y, de paso, que guarde el texto pendiente del editor. Si la pestaña se descarga, se recoloca al recargarla. Rust solo guarda los números: la página puede ser una web.
- **Espacio de trabajo por bóveda** en la configuración (`workspaces`): pestañas, división y lado con el foco. Sustituye a `last`, que solo se lee si la bóveda aún no tiene espacio de trabajo.
- **Barra oculta por defecto** (`bar_shown`, antes `bar_hidden`). Los botones de anotaciones viven en la fila de pestañas, que siempre se ve.
- **Ctrl+| además de Ctrl+\** para dividir: con el teclado latinoamericano del usuario, «\» necesita AltGr.

## Comportamiento

- **Vigilancia de archivos**: `notify` también avisa al *abrir* un archivo; esos eventos se descartan o servir una página la recarga sin fin. Las escrituras propias se reconocen por el hash del contenido escrito, no por una ventana de tiempo, para no tapar cambios de un agente.
- **Enlaces**: http/https navegan dentro de visor, `mailto:` va a `xdg-open`, el resto se ignora. `target="_blank"` abre una pestaña nueva a la vista; Ctrl+clic y clic central, una al final de la fila que no se carga hasta mostrarla (no gasta memoria). Se detectan en `decide-policy`, como hace Epiphany.
- **Pantalla completa** oculta el panel, las pestañas, la barra y el otro lado, y restaura el panel solo si estaba visible.
- **Tema**: se lee de `~/.local/state/omarchy/current/theme/colors.toml` y se vigila esa carpeta. Las páginas reciben `--visor-*` como hoja de estilo de usuario, que su propio CSS siempre puede sobrescribir. `fc-match` (fuente monoespaciada) corre aparte porque tarda ~20 ms. La transparencia de la ventana la decide el usuario en Hyprland.
- **Anotaciones**: el texto se ancla por cita con contexto (prefijo y sufijo, como Hypothesis) y el elemento por id, selector CSS, etiqueta y texto. Rust es el único que escribe `.visor/anotaciones/`.

## Editor de notas

- **CodeMirror 6 ligero con vista previa en vivo**, como Obsidian, en lugar del `<textarea>`. La lectura no cambia: Ctrl+E alterna con el editor, y CodeMirror solo se descarga al entrar en edición.
- **Sin `@codemirror/lang-markdown`**: arrastra los analizadores de HTML, CSS y JavaScript (150 KB). El lenguaje se crea con `@lezer/markdown` + GFM y `new Language(...)`; las fórmulas, los `[[enlaces]]` y el front matter son extensiones propias en `src/editor.js`. El código se colorea con highlight.js de `assets/`, no con lenguajes de CodeMirror.
- **Cifras del prototipo**: 90 KB con brotli. Con el editor abierto, +15 MB con una nota normal y +19 MB con una de 3 240 líneas; sin abrirlo, nada. Listo en 40–52 ms frente a 33–84 ms del textarea en la nota normal, y en 46–58 frente a 79–101 en la larga.
- **Medido con el editor terminado** (2026-10-07, release, `edicion.md`, 6 rondas): arranque y memoria al leer, iguales; con el editor abierto, 334–335 MB frente a 309–310 MB del textarea (+25 MB). Binario +100 KB (paquete 89 KB y `editor.js`/`editor.css` 10 KB, con brotli).
- **Regenerar el paquete**: en `tools/codemirror/`, `npm ci && npm run build` escribe `assets/codemirror/codemirror.min.js`. Las versiones están fijadas en `package.json` y `package-lock.json`. `entry.js` exporta en `window.CM` solo lo que usa `src/editor.js`; si este necesita algo más, añádelo allí y vuelve a generar.

## Compilación y peso

- `panic = "abort"` y `opt-level = 3`. `"s"` ahorraba 380 KB pero renderizaba Markdown un 8–15 % más lento; `"z"`, un 50 %.
- Las librerías de `assets/` van comprimidas con brotli: WebKit solo anuncia gzip por HTTP, pero descodifica brotli. `build-override` optimiza el script de compilación (sin optimizar, brotli tardaba 39 s).
- **Mermaid completo** (1,15 MB comprimido): la versión *tiny* ahorra 0,6 MB pero pierde los mapas mentales. El usuario eligió el completo.

## Requisitos del sistema

Vídeo web y MSE necesitan `gst-libav` (AAC, H.264) y `gst-plugins-bad` (`opusparse`). Recomendados: `gst-plugin-va` (decodificación por GPU) y `gst-plugin-dav1d`.
