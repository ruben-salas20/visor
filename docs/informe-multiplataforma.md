# Informe: visor en Mac y Windows

Fecha: 2026-10-11. Estado: análisis, sin código cambiado.

## Decisiones del usuario

- **Mac queda fuera por ahora.** No hay cuenta de Apple Developer (99 USD al año) y no se piensa pagar. Sin firma ni notarización, Gatekeeper avisa y el usuario tiene que abrir la app a mano desde Ajustes. Se puede retomar más adelante.
- **Windows es el objetivo.** Hay un equipo Windows para probar.
- **Se acepta perder parte del rendimiento actual** si no hay otra opción. La regla de medir sigue vigente: se mide y se documenta el coste, pero ya no bloquea la elección.

## Resumen

Se puede, pero no es un port: hay que reescribir la interfaz. Alrededor de un 70 % del código (bóveda, Markdown, anotaciones, agentes, servidor HTTP, modelo de pestañas) se reutiliza casi tal cual. Lo que hay que rehacer es la capa de ventana, que hoy es GTK y WebKitGTK de arriba abajo. Además hay unas diez piezas pequeñas que asumen Linux.

## 1. Lo que se reutiliza

- `vault/` (salvo lo del apartado 3), `http.rs`, `session.rs` y `agent/command.rs`, con sus esquemas CLI y MCP.
- `ui/tabs.rs` y `ui/watch.rs`, que ya están separados de GTK.
- Los JS y CSS de `src/page/`, Excalidraw y CodeMirror.
- `notify`, `rfd`, `dirs`, `tiny_http`, `pulldown-cmark`: son multiplataforma.

## 2. Lo que hay que rehacer: la capa de ventana (~3.600 líneas)

Son `app.rs`, `sidebar.rs`, `panes.rs`, `bar.rs`, `icons.rs` y `page_view.rs`.

En Linux, wry mete el webview en un contenedor GTK (`build_gtk`). En Windows no existe ese contenedor: cada webview es una vista hija que se coloca a mano con `set_bounds`. Desaparecen `gtk::Paned`, el `TreeView` del panel, las barras y los iconos de cairo.

| Opción | Qué es | Coste |
|---|---|---|
| A. Interfaz en HTML | Panel, pestañas y barra pasan a ser un webview más, con la lógica de `tabs.rs` detrás. | Contradice el ADR 0003: un proceso web más, unos +60 MB (hoy: 313 → 246 MB al quitarlo). Es la más rápida de hacer. |
| B. Toolkit de Rust (egui, iced, slint) | La interfaz se dibuja con el toolkit y los webviews van encima. | Rehacer ~3.600 líneas, incluido el árbol de carpetas y el arrastre desde el panel. Hay que medir peso y CPU en reposo. |
| C. Nativo por sistema | Win32 o WinUI en Windows y GTK en Linux. | El mejor rendimiento y tres veces el mantenimiento (dos, sin Mac). |

Pendiente: prototipar A y B y medirlas con `docs/pruebas.md` antes de elegir. Con Mac fuera, C se reduce a dos interfaces (GTK y Win32), pero sigue siendo la más cara de mantener.

## 3. Lo que asume Linux en el código

- **Socket de agentes.** `UnixListener` y `UnixStream` en `agent/socket.rs`, `server.rs` y `cli.rs`. En Windows no están en la biblioteca estándar. Alternativas: tuberías con nombre (crate `interprocess`) o TCP en localhost con un token en un archivo de permisos restringidos.
- **Dirección 127.0.0.2.** Es el origen aparte de los HTML sueltos. En Windows todo `127.0.0.0/8` funciona, así que no bloquea. Si algún día vuelve Mac, no servirá: macOS solo configura `127.0.0.1`.
- **Carpetas de estado.** `dirs::state_dir()` devuelve `None` en Windows, y `paths.rs` cae en el directorio temporal. El registro de eventos y la marca de actualización se perderían.
- **`std::os::unix` en 8 archivos de producción.** `vault.rs` usa `MetadataExt` (dispositivo e inodo, para detectar bucles) y `PermissionsExt` (en `write_atomic`). Hay que sustituirlo, por ejemplo con el crate `same-file`. Los enlaces simbólicos exigen privilegios en Windows. Las rutas con `\`, los nombres reservados (`CON`, `NUL`) y el sistema de archivos que no distingue mayúsculas afectan a `vault::resolve`.
- **Reinicio.** `app.rs` usa `exec()`, que no existe en Windows. Hay que lanzar un proceso nuevo y salir.
- **Procesos externos.** `xdg-open` pasa a `start`. `sha256sum` pasa a `certutil`. `fc-match` no existe, y haría falta una fuente monoespaciada del sistema.
- **Tema.** Lee `~/.local/state/omarchy/current`. Fuera de Omarchy hay que detectar el modo claro u oscuro del sistema.
- **Actualización automática.** El asset es `visor-x86_64-linux` y rechaza otras arquitecturas. En Windows no se puede sobrescribir el `.exe` en ejecución: hay que renombrarlo.

## 4. Funciones que hoy usan WebKitGTK directamente (ADR 0033)

wry 0.57 no cubre varias; en Windows habría que usar la API de WebView2 por debajo.

- **Frontera de confianza.** Se apoya en el evento `Committed`. wry solo ofrece `Started` y `Finished` (`with_on_page_load_handler`). Hay que enganchar el equivalente de WebView2 (`NavigationCompleted` o `ContentLoading`) y comprobar que da la URL del documento cargado. Es la pieza que más cuidado necesita.
- **Capturas** (`visor screenshot`). wry no tiene captura; WebView2 ofrece `CapturePreview`.
- **Fallos de recursos.** `connect_resource_load_started` no tiene equivalente simple.
- **Hoja de estilo de usuario.** Se puede sustituir por un script de inicialización que inyecte el CSS.
- **Botones laterales del ratón, pantalla completa, política de navegación.** Los botones se resuelven con eventos JS. La navegación, con `with_navigation_handler` y `with_new_window_req_handler`. La pantalla completa no está comprobada.
- **Vídeo.** WebView2 trae sus códecs: el ADR 0032 (GStreamer) deja de aplicar.
- **Runtime.** WebView2 viene con Windows 11. En Windows 10 hay que instalarlo.

No comprobado: cuánto cuesta cada webview en memoria en WebView2 (el ADR 0002 está medido solo en WebKitGTK) y si los iframes de otro origen se limitan a 30 fps.

## 5. Distribución, rendimiento y pruebas

- **Compilar.** Sin compilación cruzada razonable. CI con un ejecutor de Windows (GitHub Actions), y la prueba manual en el equipo Windows.
- **Firma.** Sin firma, SmartScreen avisa. Un certificado de firma de código tiene coste propio; queda por decidir.
- **Empaquetado.** MSI o instalador. `tools/publicar.sh` y la actualización automática solo cubren Linux.
- **Medidas.** `tools/medir/medir.py` usa PSS, que es de Linux. Hace falta un método para Windows.
- **Pruebas.** Varias usan `symlink` de Unix y hay que protegerlas con `#[cfg(unix)]`.

## Orden propuesto

1. Preparación en Linux, sin tocar la interfaz: aislar socket, rutas, enlaces y procesos externos detrás de un módulo `platform`. Se mide aquí y no debería cambiar nada visible.
2. Prototipos de interfaz (A y B) en Windows, medidos contra las cifras actuales.
3. CI, empaquetado y actualización para Windows.

## Preguntas abiertas

- ¿Un solo `.exe` portátil o instalador?
- ¿Se firma el ejecutable en Windows (coste del certificado) o se acepta el aviso de SmartScreen?
- ¿Interfaz en HTML (A) o toolkit de Rust (B)? Se decide con el prototipo.
