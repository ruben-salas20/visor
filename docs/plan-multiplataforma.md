# Plan de investigación: visor para cualquier persona, en Linux y Windows

Fecha: 2026-10-16; actualizado el 2026-10-19. Sale del [informe](informe-multiplataforma.md) y de seis investigaciones en paralelo. Este documento está escrito para que una sesión nueva pueda retomarlo sin el historial de la conversación.

## Cómo se usa este plan

**No es un plan de construcción.** Es una lista de experimentos pequeños, cada uno con una hipótesis, un criterio de éxito y un criterio de fracaso. Hasta que un experimento no pasa, no se escribe código de producción. Cada resultado se anota en el registro de descartes (apartado final), y lo descartado no se vuelve a discutir sin datos nuevos.

**Al principio todo salió de leer código, documentación y páginas de paquetes. Desde el 2026-10-16 hay resultados de CI en Windows y en otras distros (apartado «Resultados»).** Marcas: [V] verificado leyendo; [S] supuesto, a probar. Los informes de los agentes no se han contrastado uno a uno: trata sus cifras como pistas, sobre todo las versiones de glibc por distro (de memoria) y la política de Homebrew (resúmenes de buscador).

Reglas del proyecto que se mantienen:

- Objetivo del usuario: que cualquier persona pueda instalar visor en varios sistemas. Acepta perder rendimiento y peso si hace falta, pero cada experimento mide lo que pueda con `docs/pruebas.md`.
- Mac queda fuera por ahora (el usuario no pagará la cuenta de Apple).
- **Sin firma de código en Windows por ahora:** el usuario no pagará y se acepta el aviso de SmartScreen. No se tramita ninguna firma.
- La frontera de confianza de `AGENTS.md` no se relaja en ninguna plataforma. Si un experimento muestra que no se puede mantener, esa vía se descarta.
- Los prototipos viven en `spikes/` dentro del worktree, son desechables y no se mezclan con `src/`.
- El usuario quiere que **Windows no dependa de él**: los experimentos se ejecutan en CI siempre que se pueda.

## Para la sesión nueva (empieza aquí)

- **Dónde:** worktree `/home/ruben/Projects/visor-estudio/.claude/worktrees/port-multiplataforma`, rama `worktree-port-multiplataforma`, ya subida a GitHub. Se fusionó con `main` el 2026-10-19, después de las varias ventanas (#39). Si `main` ha avanzado, `git merge main` (no `rebase`: la rama ya está publicada).
- **Qué leer:** `AGENTS.md`, `CONTEXT.md`, este plan, `docs/pruebas.md` y el ADR 0040 (interfaz en HTML y winit en Windows). También 0001, 0035 y 0039 si se tocan la ventana, la actualización o las ventanas.
- **Estado (2026-10-22):** la interfaz en HTML funciona entera en Linux, medida frente a GTK, y en Windows con winit y WebView2, probada en CI (ver «Resultados»).
  - Puertas G1, G2 y G3 decididas. U1 funciona en Linux y en Windows.
  - Costuras hechas: lo propio de cada sistema está en `src/platform/` (`linux.rs`, `windows.rs`) y `agent/channel.rs` (socket Unix en Linux, `agent/pipe.rs` en Windows).
  - Windows sin la interfaz, hecho: tubería con nombre (con comprobación del usuario dueño), archivos, rutas, procesos y actualización. `cargo test` en Windows sin la interfaz: 265 de 265 (ejecución 39264610352). En Linux, 319 de 319.
  - L5 hecha: `.deb` y Flatpak (`io.github.ruben_salas20.visor`) funcionan; las copias de paquete no se actualizan solas (`src/package.rs`).
  - Interfaz: fase 0 y fase 1 fusionadas. Las cinco áreas (A panel, B pestañas y lados, C barra, D buscador, E ajustes, atajos y barra de estado) están en el marco, también en inglés. W4 hecho.
  - ADR 0035 ampliado; ADR 0040 nuevo.
  - CI: `windows-check.yml` (W1) pasa `cargo check` y `cargo test` en Windows sin la interfaz en cada subida a esta rama o a `port-*`; `windows-update.yml`, `windows-u1.yml` y `linux-packages.yml` cubren sus prototipos.
- **Meta: visor 0.3.0, con Linux y Windows a la vez** (decisión del usuario, 2026-10-23). Pasos, por orden:
  1. **Release compilado en CI, hecho** (fila «Release en CI»). Falta subir la versión en `Cargo.toml` (`0.3.0-rc.1` para la de prueba) y, tras fusionar en `main`, quitar el disparo por `port-release` de `release.yml`. Antes: Instalador solo para el usuario, en `%LOCALAPPDATA%\Programs\visor` (usuario, 2026-10-23). `tools/publicar.sh` solo crea la etiqueta; un flujo de CI compila y sube al release: Linux (binario de la actualización, `.deb` con su `visor.desktop` fuera de `spikes/`, Flatpak) y Windows (portable: `.zip` con `visor.exe` y `visorw.exe`; instalador: acceso directo a `visorw.exe`, asociación con `visorw.exe open "%1"`, runtime de WebView2). Flathub, sin red al compilar, puede ir después.
  2. **Riesgo de la actualización en Windows, mitigado (2026-10-23):** si fallan a la vez el renombrado del `.exe` nuevo y la vuelta atrás, `update.rs` copia el `.old` a `visor.exe` (un `.exe` en marcha se puede leer); si eso también falla, `visorw.exe` devuelve `visor.exe.old` a su sitio al abrirse (prueba de humo, caso 7). Las órdenes `visor` de los agentes no lo arreglan: hasta que se abra visorw, faltaría `visor.exe`.
  3. **Huecos de Windows.** Los que bloqueaban la 0.3.0 están hechos (fila «Huecos de Windows»): botón «Abrir carpeta…» de la página vacía, atajos propios de WebView2 (Ctrl+P) y botones laterales del ratón. Pasan a la 0.3.1 los demás. Lista completa: Fallos de red de los recursos (dominio `Network` de DevTools, coste sin medir); botones laterales del ratón por `Action::Back`; Ctrl+clic en segundo plano; `open_elsewhere` puede reabrir visor si es el programa por defecto; botón «Abrir carpeta…» de la página vacía sin comprobar; atajos propios de WebView2 (Ctrl+P) activos; carpeta de datos de WebView2 compartida entre dos instancias. Los que no bloqueen, a la 0.3.1.
  4. **Versión de prueba `0.3.0-rc`** (pre-release de GitHub). El usuario la prueba en su Windows 10/11 (clic, arrastre, teclado, escala 125–150 %, varias ventanas, reinicio, papelera, actualización, instalador y portable) y en su Linux de cada día. X11 real (Mint): si no se consigue, aviso de que no está probado.
  5. **Preparar `main`:** quitar de la rama los prototipos de `spikes/` y los flujos de CI de experimentos (quedan en la historia); ADR 0001 y 0004 al día (con consulta); PR a `main`.
  6. **Publicar la 0.3.0.**
  - Sin decidir: CSP para el origen del marco.
  - Hecho antes: pendientes de la fase 0 (2026-10-22: resueltos; las páginas HTML sueltas y las webs van sobre fondo blanco), anfitrión de Windows y `visorw.exe` (filas en «Resultados»).
- **Al terminar cada subagente:** revisar su diff, fusionar en esta rama, anotar en «Resultados», y borrar su worktree y sus ramas (local y de GitHub). Encargos acotados: un subagente Haiku se quedó sin contexto en U1; el de Opus, con líneas exactas que leer, terminó en 15 minutos.
- **Permisos:** subir la rama del worktree y lanzar su CI está autorizado. **Sin permiso explícito:** tocar `main`, instalar nada con `sudo`, cambiar ADR, comentar en el issue 35 (el repositorio es público), abrir ventanas en el escritorio del usuario (Hyprland) y medir rendimiento. Las pruebas funcionales en un sway sin pantalla y `cargo test` no necesitan permiso.
- **Prototipos útiles:**
  - `spikes/u1-marco-html/`: marco en HTML con dos páginas. `--marco simple|react|svelte` elige la implementación (fuentes en `marcos/`, compilados en `web/`; `npm install && npm run build` en cada `marcos/*`). `pruebas/ejecutar.sh --marco X` pasa las pruebas a–g en un sway sin pantalla; `pruebas/medir.py` mide los tres marcos y `pruebas/medir_visor.py VISOR` los compara con visor.
  - `spikes/u2-toolkit-rust/`, `spikes/w2-webviews/`, `spikes/w3-committed/`, `spikes/w1-check/`, `spikes/linux/`: ver su README o la cabecera de sus scripts.
- **Lecciones de esta fase:**
  - El sway sin pantalla no tiene ratón (`swaymsg -t get_seats` muestra `seat0` sin dispositivos): el clic, el arrastre y parte del foco solo se prueban a mano en el escritorio del usuario, con su permiso. `wtype` sí da teclado.
  - Dos agentes en paralelo no deben escribir en la misma carpeta con los mismos nombres de archivo: en U1 se cruzaron las capturas de React y Svelte.
  - wry crea un contexto web por vista si no se le pasa uno; visor comparte uno. Para comparar memoria, compartir siempre un `WebContext`.
  - El prototipo U1 valida `--marco` después de abrir la ventana: no lanzarlo fuera del sway sin pantalla para probar opciones.
- **Probar en esta máquina:** el usuario suele tener visor abierto. Aislar siempre con `VISOR_SOCKET` y `XDG_*` propios (ver `docs/pruebas.md`) y cerrar por PID.
- **Idioma:** textos de interfaz en español; commits y títulos de PR en inglés; comentarios del código en inglés y solo para el porqué (`AGENTS.md`, `CONTRIBUTING.md`).

## Cómo funciona el CI (coste y límites)

- `ruben-salas20/visor` es **público y MIT**, así que los ejecutores estándar de GitHub Actions son **gratis** y sin límite de minutos [V, documentación de facturación de GitHub, consultada 2026-10-16].
- Si el repositorio pasara a privado: el plan Free da 2.000 minutos al mes; Windows cuesta 0,010 USD por minuto (Linux 0,006; macOS 0,062). Los ejecutores grandes son de pago aparte y no hacen falta.
- **Ejecutor de Windows:** imagen Windows Server 2025 con Rust 1.99, MSVC (Visual Studio 2022) y Edge instalados [V, `runner-images`]. **El runtime de WebView2 no figura en su lista de software**: el primer trabajo debe comprobar la clave del registro (`pv` de EdgeUpdate) y, si falta, instalar el bootstrapper.
- **Límites:** son máquinas virtuales de 2 núcleos, con mucho ruido para medir rendimiento. El ejecutor es Windows Server, no Windows 10/11 de escritorio. Los contenedores Linux (Docker) sí funcionan en los ejecutores de Ubuntu, así que L1 y L2 no necesitan Docker en esta máquina.
- Los flujos de trabajo van en `.github/workflows/` de la rama del worktree y se lanzan con `workflow_dispatch` o al subir la rama. Los resultados se leen con `gh run view` y `gh run download`.

## Lo que ya sabemos

- **Linux.** El binario exige glibc ≥ 2.39 solo por dos símbolos opcionales de Rust (`pidfd_spawnp`, `pidfd_getpid`) que el enlazador registró como obligatorios [V, `readelf -V`]. Deja fuera, según las páginas de paquetes, Ubuntu 22.04, Debian 12, Mint 21, RHEL 9 y Leap 15.6 [glibc de memoria]. Arranca bajo X11 vía XWayland (`GDK_BACKEND=x11`, `status` responde) [V]; el dibujo, el arrastre y los menús no se probaron. RHEL 9 no trae WebKitGTK 4.1 y Alpine usa musl [páginas de paquetes].
- **Windows.** wry da el webview, pero no `Committed`, captura, fallos de recursos ni pantalla completa [V, código de wry 0.57]. WebView2 los tiene por su API COM, accesible con `WebViewExtWindows::webview()`. La implementación de wry de `PageLoadEvent::Started` lee `Source()` y no sirve para la frontera de confianza, que prohíbe `webview.uri()` [V].
- **Interfaz.** Sin GTK en Windows hay que rehacer panel, pestañas, barra y divisores: `app.rs` ~1.785 líneas, `sidebar.rs` ~1.516 y más. Candidatas vivas: A (interfaz en HTML en un webview propio) y B (toolkit de Rust con webviews hijos).
- **Código.** El inventario completo está en este plan; lo nuevo respecto al informe es: el secreto de las URL sale de `/dev/urandom` (`http.rs:92`), `is_loose` usa `starts_with('/')` (`vault.rs:87`), `canonicalize` en Windows devuelve `\\?\C:\…`, no se validan nombres reservados de Windows, y se lee `LANG` en `i18n.rs`. `std::os::unix` aparece en 6 archivos de producción (`vault.rs`, `update.rs`, `agent/{socket,server,cli}.rs`, `ui/app.rs`). Ya son portables: `ui/tabs.rs`, `ui/watch.rs`, `ui/page_calls.rs`, `ui/action.rs`, `session.rs` y la capa de datos.

## Resultados

Ejecuciones de CI en la rama del worktree. Los prototipos están en `spikes/` y sus flujos en `.github/workflows/`.

| Experimento | Resultado | Ejecución |
|---|---|---|
| L1, compilar en Debian 12 | Compila. El binario exige glibc **2.34** (antes 2.39) sin cambiar el código. | 38943483244 |
| L2, `visor status` por distro | Funciona en Ubuntu 22.04 y 24.04, Debian 12 y 13, Fedora 42 y 43, Arch y openSUSE Tumbleweed. Falla en AlmaLinux 9 (no hay WebKitGTK 4.1) y en Alpine (musl). Solo se probó `status`. | 38944249549 |
| L3, X11 | En el portátil del usuario, con `GDK_BACKEND=x11` bajo XWayland (2026-10-16 y 2026-10-17): nota con fórmulas y diagrama, HTML, PDF, dibujo, pestañas y vista dividida, sin errores. **Falta un escritorio X11 real (Linux Mint):** fondo translúcido, escala y teclado y ratón. | Manual |
| W1, compilar en Windows sin la interfaz | Repetido tras #39, igual que antes: 21 errores (4 de interfaz y 17 de `std::os::unix`); 18 tras rellenar las costuras y quitar los `use` de Unix. Todos están en `agent/{socket,server,cli}.rs`, `vault.rs` y `update.rs`: socket Unix, `dev`/`ino`, `from_mode` y `process_group`. Costuras con la interfaz: la papelera de GTK (`vault/files.rs:56`), `tao` en `ui/action.rs` y `ui::page_view` en `tabs.rs` y `address.rs`. `desktop.rs` (#25) compila, pero solo tiene sentido en Linux. | 39238287661 |
| W2, varios webviews en WebView2 | Con winit: N=1, 97 MB privados y 6 procesos; N=5, 194 MB y 10 procesos; N=10, 321 MB y 15 procesos. Unos **25 MB por webview extra**. Redimensionar funciona. Un entorno compartido no ahorra nada. **Con tao, el bucle se bloqueó** con varios webviews hijos: 2 de 10 en el ejemplo de wry portado a tao, siempre en nuestro prototipo; con winit, 0 de 10. | 38949242448, 38949714000 |
| U1, marco en HTML (opción A) | En sway sin pantalla: marco y dos páginas, pestañas, abrir desde el árbol, recolocar al redimensionar y una página hostil (8 de 8 órdenes rechazadas). En Hyprland, a mano (usuario, 2026-10-19): clic, arrastre del árbol a una página, Ctrl+W y redimensionar funcionan. Fallo del prototipo: al encoger la ventana, el contenido sale estrechado y queda una franja vacía abajo (causa probable: `set_size_request` actúa como tamaño mínimo). Sin medir. `spikes/u1-marco-html/` | Manual |
| U1, coste de cada marco | 2026-10-19, enchufado, perfil `performance`, 3 rondas en sway sin pantalla (`spikes/u1-marco-html/pruebas/medir.py`); marco más dos páginas vacías, 7 procesos. Mediana (rango). Marco pintado: simple 159 ms (158–167), React 203 ms (199–204), Svelte 206 ms (205–216). Memoria PSS: simple 316 MB (316–318), React 363 MB (356–363), Svelte 371 MB (369–373). CPU en reposo: 0,07–0,13 % en los tres. Sin comparar todavía con visor (GTK) en las mismas condiciones. | Local |
| U1 frente a visor | 2026-10-19, mismas condiciones, 3 rondas (`pruebas/medir_visor.py`), la misma página en los dos lados, un contexto web compartido. Memoria PSS: visor (GTK) 213–215 MB y 4 procesos; marco simple 266–270 MB (+54) y 5 procesos; marco React 309–324 MB (+97 a +110) y 5 procesos. Apertura hasta cargar las dos páginas: visor 286 ms (277–304, incluye dos llamadas a la CLI y espera al evento `load`); simple 207 ms; React 216 ms. La apertura no es comparable uno a uno: el prototipo no hace lo que visor hace al arrancar. Decisión del usuario: React, cargando solo los componentes que se ven. | Local |
| Fase 1, costuras | Lo propio de cada sistema pasa a `src/platform/` (`linux.rs`, `windows.rs`: identidad de archivo, papelera, proceso suelto, reinicio y permiso de ejecución) y a `agent/channel.rs` (socket Unix; en Windows, `agent/pipe.rs`). `action.rs` recibe una función de envío en lugar del `EventLoopProxy` de tao, y `is_web` queda una sola vez en `vault.rs`. `cargo check` para Windows sin la interfaz (`spikes/w1-check/prepare.py`): **0 errores** (antes 21). Los rellenos de Windows son `todo!()`, salvo el permiso de ejecución, que no existe allí. Linux: 290 pruebas correctas, clippy sin avisos, binario +336 bytes (5.327.240 → 5.327.576). Pendiente en código que ya compila: el secreto lee `/dev/urandom` (`http.rs`) y `update.rs` usa `curl` y `sha256sum`. Confirmado en el ejecutor de Windows: 0 errores, cargo correcto. | 39258306655 |
| Fase 2, B: archivos y procesos en Windows | `platform/windows.rs`: identidad de archivo (serie del volumen + índice), papelera con `SHFileOperationW`, proceso suelto (`DETACHED_PROCESS`), reinicio (lanza y sale; el nuevo espera hasta 5 s a que el viejo suelte la tubería, con `VISOR_RESTART`). `vault.rs`: en Windows rechaza `C:`, `\\servidor`, `\\?\`, `\` como separador, nombres reservados (`CON`, `NUL`…), caracteres prohibidos, punto o espacio final y nombres cortos 8.3 que esconden una carpeta oculta; `canonicalize` sin el prefijo `\\?\`; uniones que salen de la bóveda no se recorren. `state_dir` en Windows es `data_local_dir`. 251 de 251 pruebas en Windows. Sin probar: `detach` y `restart` en un escritorio real; rutas largas en la papelera. Resuelto después (decisiones del usuario, 2026-10-19): `Walk` salta lo que Windows marca oculto o de sistema; un nombre corto 8.3 (`OCULTA~1`) no abre una carpeta oculta (comprobado con nombres cortos activos en el ejecutor; 265 de 265, ejecución 39264610352); en Linux y Windows, un enlace a una carpeta oculta de la bóveda ya no se lista ni es una entrada del panel. Rama `port-files`, fusionada. | 39262094015 |
| Fase 2, A: canal de la CLI en Windows (W5) | `agent/pipe.rs`: tubería con nombre `\\.\pipe\visor-<hash de paths::socket()>`, instancia única (`FILE_FLAG_FIRST_PIPE_INSTANCE`), DACL solo para el SID del usuario, `PIPE_REJECT_REMOTE_CLIENTS`, una instancia por cliente, cliente con `SECURITY_IDENTIFICATION`. El servidor cierra sin `FlushFileBuffers`, para que un cliente que no lee no bloquee un hilo. 5 de 5 pruebas en Windows. Sin probar: un segundo usuario real. **Riesgos resueltos (2026-10-19):** la CLI y `claim` comprueban que el proceso servidor es del mismo usuario (`GetNamedPipeServerProcessId`), así que otro usuario no puede suplantar a visor (solo se probó el caso bueno; el malo, por revisión de código; ejecución 39264412121); un visor reiniciado espera a que el anterior suelte la tubería (`VISOR_RESTART`). Rama `port-pipe`, fusionada. | 39261123326 |
| Fase 2, C: actualización en Windows | `update.rs`: SHA-256 con `sha2` en Windows (Linux sigue con `sha256sum`); recurso `visor-x86_64-windows.exe`; el `.exe` en marcha se renombra a `visor.exe.old` y el nuevo ocupa su sitio, con vuelta atrás si falla; el `.old` se borra al arrancar. `curl.exe` existe en el ejecutor. 11 de 11 pruebas de `update` en Windows. Pendiente: `tools/publicar.sh` no sube el `.exe`; el ADR 0035 dice `sha256sum` y `rename` atómico (consultar); si falla también la vuelta atrás, no queda `visor.exe`. Rama `port-update`, fusionada. | 39260977665 |
| L5, paquetes de Linux | `.deb` (cargo-deb, compilado en Debian 12): se instala en Ubuntu 22.04, 24.04 y Debian 12 limpios, apt resuelve las dependencias (203–224 paquetes en un sistema mínimo); 3,1 MB. Flatpak (`io.github.ruben_salas20.visor`, runtime GNOME 51, que trae WebKitGTK 4.1): arranca, abre una nota de `$HOME`, `visor mcp` responde; bundle de 3,1 MB sin contar el runtime. Los dos: no escriben su entrada de escritorio y `update` dice que lo actualiza el paquete. CLI desde fuera, mediana de 10: `.deb` 24–30 ms; Flatpak 70–123 ms con `flatpak run` (el socket compartido en `$XDG_RUNTIME_DIR/app/<id>/` funciona). Flatpak no ve el `/tmp` del sistema (esperado). Permisos: `ipc`, `wayland`, `fallback-x11`, `dri`, `filesystem=home`. Sin probar: escritorio real (solo Xvfb), tamaño del runtime. Para Flathub faltaría compilar sin red (dependencias de Rust incluidas en el manifiesto). `spikes/l5-paquetes/`. **Flatpak supera la condición del plan.** | 39263234768 |
| U1 en Windows (winit + WebView2) | Marco React y páginas en webviews hijos de una ventana winit, un entorno web compartido, en el ejecutor de Windows. a) Marco y dos páginas cargan: 15 de 15, **0 cuelgues**. b) Página hostil: 8 de 8 órdenes rechazadas con su motivo, estado del marco sin cambios, URL confirmada por COM sigue en `/hostil.html`; navegación externa y `window.open` bloqueados. c) Memoria: marco + 2 páginas, 9 procesos y 166 MB privados; marco + 5, 12 procesos y 243 MB; **unos 25 MB privados por página extra** (55 MB de working set), un proceso de render por página. d) Redimensionar a más grande y a más pequeño: los bounds coinciden al píxel (al encoger funciona, a diferencia del prototipo de Linux). Sin probar: clic, arrastre y teclado reales, lector de pantalla, Windows 10/11 de escritorio, escalas de 125–150 %, orden de apilado visible. `spikes/u1-windows/` | 39265938803, 39266487153, 39266913865 |
| W4, funciones que wry no da en Windows | Las siete se pueden hacer con la API COM de WebView2. Captura (`CapturePreview`): PNG del tamaño de la vista, colores exactos. Pantalla completa: `requestFullscreen()` lanzado por el anfitrión entra sin gesto del usuario; Rust recibe `ContainsFullScreenElementChanged` al entrar y salir y debe agrandar la vista a mano. Zoom: `wry::zoom` basta y se conserva al navegar. Foco: `GotFocus`/`LostFocus` dicen qué vista lo tiene; `wry::focus()` lo da. Botones laterales: `SendInput` llega en el ejecutor; se cancelan en `NavigationStarting` (tipo `BACK_OR_FORWARD`), pero eso cancela también el «atrás» de visor: hará falta una marca. **Con límites:** fallos de recursos necesitan `WebResourceResponseReceived` (HTTP) y el dominio `Network` del protocolo DevTools (red); no cubren errores de red en iframes de otro sitio, y hay que filtrar el `/favicon.ico` de Chromium. Hoja de estilo: `AddScriptToExecuteOnDocumentCreated` llega a todos los marcos y se cambia en caliente, pero entra al nivel de la página: un `!important` de la página gana al tema (en WebKit, no). Aviso: esperar dentro de un evento de winit con un bucle de mensajes sin tope se cuelga (`WM_PAINT`); visor debe responder desde el bucle. Sin probar (W7): clic real, ratón físico, vídeo a pantalla completa, escalas distintas de 1,0, coste del dominio `Network`. `spikes/w4-funciones/` | 39273048404, 39273561199 |
| Diseño del marco | Tres maquetas (A, el prototipo Svelte; B, compacta tipo editor; C, aireada tipo app de notas) y una cuarta, **D, elegida por el usuario**: la B (pestañas planas con línea de acento arriba, ruta por partes, barra de estado con ajustes, zoom, atajos y versión, búsqueda flotante) con el árbol y la cabecera de panel de la A (13 px, filas de 24 px, nombres largos que se desvanecen antes del número), cabecera de 30 px como las pestañas. Los colores del tema se mezclan con `color-mix` a partir de los cinco de `theme.rs`. Capturas con WebKitGTK en sway sin pantalla (`capturar.sh`): Obscura no dibuja bien. `spikes/maquetas-marco/` (`npm install && npx vite`, `?v=a|b|c|d&tema=actual|claro&estado=normal|ajustes|buscar`) | Local |
| Fase 0 de la interfaz | La interfaz GTK (panel, pestañas, barra, buscador, atajos, iconos de cairo) se sustituye por el marco React (`marco/`, diseño D, compilado en `assets/marco/`). Anfitrión de Linux en `ui/host.rs`: el marco ocupa la ventana y cada lado se coloca a mano tras el reparto de GTK; al encoger ya no queda franja vacía. Protocolo en `ui/protocol.rs` (`FrameMsg` por `area` y `type`; `ToFrame` con instantáneas que se envían solo si cambian) y `marco/src/protocol.ts`; todo el transporte pasa por `ui/frame-bridge.js` (#48). Frontera: el marco tiene su origen, 127.0.0.255 con prefijo secreto, una clave que solo conoce el puente y su propio manejador IPC; `accept_frame` exige la URL registrada en `Committed`; los mensajes de página van por `type` y nunca por `area` (7 pruebas). Comprobado en sway sin pantalla: árbol, pestañas, vista dividida, dos ventanas, página suelta, CLI. `cargo test` 290 de 290, clippy sin avisos, `cargo check` para Windows sin errores en local. Binario +71 KB (5.330.952 → 5.402.280); JS del marco, 296 KB sin comprimir. Sin medir arranque, memoria ni CPU. ADR afectados, sin cambiar: 0001, 0003, 0004, 0006, 0007, 0014, 0018, 0024, 0034. | Local; CI 39279750752 en cola |
| Menús encima de las páginas | Cuatro vías (`spikes/menus-encima/`). **Elegida (usuario, 2026-10-19): el marco encima de las páginas**, transparente en los huecos, con la entrada limitada a la ventana menos los huecos más los recuadros que declara el marco (menús ampliados 24 px por la sombra; la ventana entera mientras se arrastra). Linux: 11 de 11 pasos a escala 1, 1,25 y 1,5; Windows (región con `SetWindowRgn`, WebView2 mezcla vistas hermanas transparentes): 11 de 11. Descartadas: tercer webview para emergentes (sin ventaja, +25 MB en Windows), emergentes nativos (un webview por emergente o fuera del diseño D), páginas en iframes (rompe la frontera: sin aislamiento de proceso en WebKitGTK, sin `Committed` ni identidad por iframe). Llevada a `src/`: `ui/cover.rs` (zona, sin GTK), mensaje `cover`, `marco/src/overlay.ts` (los componentes flotantes de `components/ui/` se declaran solos; `useOverlay` para los propios); fuera las zonas GTK para soltar de B. Sin probar: ratón físico, escalas en Windows, lector de pantalla, **CPU al repintar el marco encima** (el usuario lo prueba antes de medir). | 39289889833, 39303474586 |
| Fase 1 de la interfaz | Cinco ramas, una por área, fusionadas el 2026-10-19: A panel (`port-area-a`), B pestañas que reparten el ancho, divisor, zonas para soltar y pantalla completa (`port-area-b`), C barra, D buscador sobre la página del lado con foco (`port-area-d`), E ajustes, atajos (tabla por grupos, Ctrl+K) y barra de estado (`port-area-e`). Después: textos en inglés en el idioma que manda Rust (`port-i18n`), la barra de dirección siempre visible (fuera Ctrl+Shift+B), y Ctrl+L ya no pierde la primera tecla (2026-10-20). Una prueba exige que cada atajo figure en `help.txt`. `cargo test` 319 de 319 (2026-10-22). | Local |
| Interfaz en HTML frente a GTK | 2026-10-19, `96ff50d` frente a `main`, dos rondas de `medir.py` (detalle en `docs/pruebas.md`, «Interfaz en HTML»). Arranque en frío: igual (ruido). Cambio de página −2 ms; pestaña nueva −5 ms. Memoria: +74 a +90 MB (el webview del marco y React, aceptado en el ADR 0040). CPU en reposo: igual. CPU mientras la página se mueve: Three.js +6 puntos (18–20 % → 25–26 %), desplazamiento continuo +4 puntos (8–9 % → 12–13 %), porque GTK compone la capa del marco en cada fotograma. Binario +250 KB. | Local |
| Anfitrión de Windows | Rama `port-host-windows` (subagente Opus), fusionada el 2026-10-22. Costuras nuevas con mitad `linux.rs`/`windows.rs`: `ui/shell` (bucle, ventanas, temporizador, contexto web y tabla común de atajos), `ui/host` (en Windows, vistas colocadas en píxeles físicos y zona del marco con `SetWindowRgn`) y `ui/page_view` (`build_page`, `PageView`, `Finder`). URL confirmada en `ContentLoading`, emparejada por `NavigationId` con `NavigationStarting`; las navegaciones canceladas no se registran. WebView2 mete los scripts en todos los marcos: en Windows van dentro de `if (window.top === window)`. `bridge.js` usa `window.chrome.webview` si no hay `webkit`. Entraron también pantalla completa, zoom, foco, atajos (`AcceleratorKeyPressed` en cada vista), hoja del tema por script, errores HTTP de recursos, buscador en webs (`ICoreWebView2Find`) y recarga del marco si cae. Prueba de humo (`tools/windows/smoke.ps1`, flujo `windows-host.yml`, que sustituye a `windows-check.yml`): 23 de 23, con clics reales, página hostil y web en otro puerto rechazadas, cierre limpio. `cargo test` en Windows 316 de 316; en Linux 323 de 323, y prueba funcional en sway sin pantalla igual que antes. Binario de Linux −240 B; de Windows 5.080.064 B. Dependencias solo de Windows: `winit 0.30`, `webview2-com 0.39`, `windows 0.62`, `windows-core 0.62`. Revisado por el padre: frontera de confianza y fusión con el fondo blanco. | 39879489835 |
| Sin consola en Windows | Rama `port-console` (subagente Opus), fusionada el 2026-10-23. `visorw.exe` (`src/bin/visorw.rs`, solo `std`, subsistema GUI) arranca `visor.exe` de su carpeta con los mismos argumentos y `DETACHED_PROCESS` (con `CREATE_NO_WINDOW` Windows crea igual un `conhost` oculto); si falta, lo dice en un aviso de Windows. `curl` va con `platform::no_console`. Las actualizaciones solo cambian `visor.exe`. CI: la app abierta con `visorw.exe` no tiene `conhost` hijo (control: `visor.exe` abierto directamente sí lo tiene), `visor status` y `visor mcp` responden, prueba de humo entera. `visor.exe` 5.082.112 B, `visorw.exe` 212.480 B; Linux +256 B (texto de `help.txt`). Sin probar: doble clic real, accesos directos (falta el instalador). El instalador debe asociar archivos con `visorw.exe open "%1"`, no `visorw.exe "%1"`. | 39992510306 |
| Release en CI | Rama `port-release` (subagente Opus), fusionada el 2026-10-23. `tools/publicar.sh` solo crea y sube la etiqueta; `release.yml` compila, prueba y publica (pre-release si la versión lleva `-`). Archivos (prueba en seco, versión 0.2.0): `visor-x86_64-linux` 5.563.400 B (Debian 12, glibc 2.34), `.deb` 3.374.768 B, Flatpak 3.376.344 B, `visor-x86_64-windows.exe` 5.082.112 B, `.zip` portable 3.697.916 B, instalador 7.115.241 B (Inno Setup, por usuario, con el bootstrapper de WebView2 firmado por Microsoft, 1,9 MB), `SHA256SUMS`. Probado en CI: `.deb` en Debian 12 y Ubuntu 22.04 limpios, Flatpak instalado, instalador silencioso (acceso directo a `visorw.exe`, «Abrir con» para `.md`, `.markdown`, `.html` y `.htm` con `open --loose`, como en Linux, sin ser predeterminado; prueba de humo entera desde la copia instalada; desinstalación limpia), contenido del `.zip`. `is_newer` ya entiende `0.3.0-rc`: quien pruebe la versión de prueba recibe la final (corregido por el padre). Sin probar: instalación en un escritorio real, la rama que instala WebView2, una actualización real entre dos versiones publicadas. Sin firma (SmartScreen avisa) y sin icono propio en los `.exe`. | 39998377073 |
| Huecos de Windows | Rama `port-huecos` (subagente Opus), fusionada el 2026-10-23. «Abrir carpeta…»: WebView2 confirma `load_html` en una dirección `data:`; la página vacía de visor se reconoce por una marca que pone `load_html` y se registra como `visor:empty` (ninguna navegación llega a ese esquema); en Linux, `about:blank`, lo que WebKit confirma con `load_html` sin `base_uri`. `accept` y `on_page` pasan de «empieza por `about:`» a esa dirección exacta. Atajos del navegador apagados (`with_browser_accelerator_keys(false)`) en páginas y marco; el inspector sigue en el menú contextual. Botones laterales: gancho de ratón de bajo nivel (`WH_MOUSE_LL`) en un hilo propio; solo actúa con XBUTTON sobre una vista de visor, la enfoca y manda `Action::Back`/`Forward`; WebView2 no ve la pulsación. A diferencia de Linux, actúa en la vista bajo el puntero, no en el lado enfocado. Coste del gancho sin medir (descarta al instante lo que no es XBUTTON). Prueba de humo, casos 8–10: diálogo de carpeta (y una web que lo pide, rechazada), Ctrl+P sin impresión, Ctrl+Shift+I sin inspector, Ctrl+A/C/V y zoom bien, XBUTTON1/2 una sola vez. Sin probar: ratón físico, distribuciones de teclado, Esc en un vídeo a pantalla completa con los atajos del navegador apagados. Riesgo: un antivirus puede recelar de un gancho global de ratón. | 39999225352 |
| U2, egui y slint con wry | Descartada en Wayland: ver registro de descartes. `spikes/u2-toolkit-rust/` | Local |
| W3, `Committed` por `NavigationId` | 14 de 14 casos correctos: redirección, `pushState`, fragmento, error de red, navegación solapada, iframe externo, carga pendiente y atrás. | 38945354780 |

## Puertas de decisión

| Puerta | Pregunta | Se decide con | Si falla |
|---|---|---|---|
| G1 | ¿Es viable Windows con la frontera de confianza intacta? | W2, W3 | Se detiene todo el port a Windows y se replantea |
| G2 | ¿Qué interfaz compartida usamos? | U1, U2 (y W2 repetido con ella) | Se vuelve a evaluar D (dos bases) |
| G3 | ¿Qué alcance real tiene Linux y qué formatos ofrecemos? | L1, L2, L3 | Se acota la lista de distros prometidas |

## Experimentos de Linux

Salvo L3, se ejecutan en CI con contenedores (ejecutores de Ubuntu), sin depender de esta máquina ni del usuario.

**L1. ¿Compila y arranca el release en una base antigua?**
- Hipótesis: compilar en Debian 12 (glibc 2.36, WebKitGTK 2.50.6 [V según páginas de paquetes]) da un binario que baja la exigencia de glibc a ~2.36 sin tocar el código.
- Cómo: contenedor Debian 12 con `libwebkit2gtk-4.1-dev`, `cargo build --release`, y `objdump -T` / `readelf -V` para ver las versiones exigidas.
- Éxito: compila con la feature `v2_38` de `webkit2gtk` y la exigencia baja.
- Fracaso: no compila o sigue exigiendo glibc ≥ 2.39. Entonces se prueba Ubuntu 22.04 y, si también falla, se acota el alcance.

**L2. ¿En qué distros arranca de verdad?**
- Hipótesis: con el binario de L1, `visor status` responde bajo `xvfb` en Ubuntu 22.04 y 24.04, Debian 12 y 13, Fedora 42 y 43, Arch y openSUSE Tumbleweed; falla en AlmaLinux 9 y Alpine.
- Cómo: contenedor por distro, instalar WebKitGTK 4.1 y `xvfb`, lanzar con `VISOR_SOCKET` propio y consultar el estado.
- Éxito: tabla con cada distro «funciona», «falla (motivo)» o «no probada». Fracaso: resultados que contradicen las tablas de paquetes; se registran igual.

**L3. ¿Funciona entero bajo X11?** (manual, el usuario o una máquina virtual con escritorio)
- Hipótesis: panel, arrastre, menús emergentes, escala y teclado funcionan igual que en Wayland.
- Cómo: lista de comprobación bajo XWayland y, si es posible, X11 real, con escala 1 y 1,5.
- Éxito: sin diferencias visibles; se cambia el aviso del README. Fracaso: fallos concretos; el aviso pasa a «Wayland probado, X11 experimental».

**L4. ¿Compila en aarch64?**
- Ejecutor ARM nativo (`ubuntu-22.04-arm` o `ubuntu-24.04-arm`) [V según el agente]. Éxito: compila y arranca. Fracaso: se anota qué dependencia falla.

**L5. Formatos de paquete.**
- `.deb` con `cargo-deb`: instalarlo en un contenedor Ubuntu 22.04 limpio y comprobar que resuelve las dependencias.
- Flatpak: ¿puede el CLI del host hablar con la app dentro del sandbox? Prueba mínima de socket en `xdg-run` y de acceso a una carpeta arbitraria. Si el socket no se alcanza sin permisos amplios, Flatpak se descarta por ahora.
- `update.rs` bajo un paquete gestionado: comprobar que se desactiva.

## Experimentos de Windows (en CI)

Cada prototipo es un proyecto Rust independiente en `spikes/` más un flujo de trabajo para `windows-2025`. Resultado: artefactos y registros de la ejecución.

**W1. ¿Cuánto del código compila ya en Windows?**
- Hipótesis: `vault/`, `agent/`, `http.rs`, `session.rs`, `tabs.rs` y `watch.rs` compilan en Windows con pocos cambios, dejando la interfaz fuera.
- Cómo: `cargo check` en el ejecutor sobre una copia con `ui/` desactivada. Se cuentan los errores por categoría.
- Éxito: errores acotados y todos de la lista conocida. Fracaso: hallazgos nuevos grandes, que se añaden al inventario.

**W2. wry + WebView2: varios webviews hijos.**
- Hipótesis: con un entorno compartido (`with_environment`), 5-10 webviews hijos cuestan ≲ 60-65 MB cada uno y se colocan con `set_bounds`.
- Cómo: `tao` + `wry`, un servidor HTTP local y 1, 5 y 10 webviews. Medir procesos y memoria de forma programática.
- Éxito: coste parecido al de WebKitGTK y sin fallos de foco ni de redimensionado. Fracaso: coste muy superior; entonces cuentan más las pestañas ocultas suspendidas (`TrySuspendAsync`) o un solo webview.
- Aviso: en CI las cifras son orientativas (máquina virtual de 2 núcleos).

**W3. La frontera de confianza: `Committed` por `NavigationId`.** (decide G1)
- Hipótesis: se puede reproducir `Committed` con `NavigationStarting` + `ContentLoading` unidos por `NavigationId`, sin leer `Source()`.
- Cómo: `webview2-com` sobre `WebViewExtWindows::webview()`. Páginas de prueba que provoquen redirección HTTP, `history.pushState`, cambio de fragmento, error de red, navegación solapada, iframe externo y carga pendiente. La prueba se registra sola y falla el trabajo si la URL no coincide.
- Éxito: la URL registrada nunca cambia por `pushState` ni por fragmento, y es la del documento realmente cargado en todos los casos.
- Fracaso: algún caso en el que la URL registrada no es la del documento. **Se detiene el port a Windows**: sin esta pieza no se puede cumplir `AGENTS.md`.

**W4. Funciones que wry no da.**
- Captura (`CapturePreview`), botones laterales del ratón (por script y, si es posible, simulados), pantalla completa, fallos de recursos de subframes, hoja de estilo por script de inicio, zoom y foco.
- Cada una: funciona / funciona con límites / no funciona. Las que no funcionen se anotan como funciones que Windows no tendría. Las que necesiten entrada real quedan para W7.

**W5. IPC del CLI.**
- Tubería con nombre con ACL al usuario actual e instancia única, frente a TCP en localhost con token. Criterio: el CLI y el MCP hablan igual que con el socket Unix; un segundo usuario no puede conectar (se prueba con un proceso con otra identidad si el ejecutor lo permite; si no, queda como revisión de código).

**W6. Runtime de WebView2 ausente.** (parcial en CI)
- Comprobar en el ejecutor la detección del runtime (clave `pv`) y el flujo del bootstrapper (`/silent /install`). No cubre un Windows 10 limpio ni el mensaje de error sin red: eso queda para W7.

**W7. Pasada final con una persona.** (manual, el usuario, al final)
- Instalar el artefacto que genere CI en un Windows 10 y un Windows 11 de escritorio: aspecto, HiDPI, ratón y teclado reales, botones laterales, pantalla completa, mensaje sin runtime y aviso de SmartScreen. Es la única parte que seguirá necesitando a una persona.
- Sobre SmartScreen: el ejecutable irá **sin firmar** (decisión del usuario, 2026-10-16). Esta pasada solo anota qué ve la persona al descargarlo y qué pasos exactos debe dar para abrirlo, para ponerlo en el README.

## Experimentos de interfaz

**U1. Prototipo A: panel, pestañas y barra en HTML.** (en Linux, ~4 h)
- Cuadro: lista de pestañas, árbol de ~200 nodos, barra, y 2 webviews de página en `gtk::Fixed`. Recibe los efectos de `tabs.rs` y envía eventos por IPC.
- Mide con `tools/medir/medir.py`: arranque, PSS del árbol de procesos, CPU en reposo y peso.
- Prueba: arrastre del árbol a una página; lectura con Orca; y que **una página no pueda alcanzar los mensajes de la interfaz** (frontera de confianza).
- Fracaso: si el arrastre sobre webviews hijos no se puede resolver o la frontera se rompe, A se descarta.

**U2. Prototipo B: toolkit de Rust (egui o slint) con webviews de wry.** (1 h de corte)
- Primero: ¿arranca en Wayland con `build_gtk` dentro de la ventana? Si no, se descarta en una hora. Si arranca: mismas medidas y pruebas que U1.

**U3. GPUI (opcional, solo para refutar).**
- Ventana GPUI con `gpui-webview` y 2 webviews en Hyprland con escala 1,5. Fuera del camino crítico: el estado actual es «descartado».

**Criterio para G2:** se elige A salvo que B ahorre más de ~50 MB y funcione en Wayland con arrastre y lector de pantalla sin parches. Después se repite W2 en CI con la interfaz elegida.

## Siguiente fase

Aprobado por el usuario: repartir la construcción en tareas paralelas con subagentes, una rama por tarea. El orden de abajo es una propuesta; el usuario elige por dónde empezar. Antes de construir, falta repetir U1 en Windows (CI, winit) para confirmar que el marco funciona allí igual.

La construcción se reparte en tareas paralelas, una rama por tarea, solo donde los archivos no se cruzan:

1. **En serie, primero (hecho el 2026-10-19, ver «Resultados»):** definir las costuras que separan lo común de lo propio de cada plataforma: canal del CLI (`agent/socket.rs`), identidad de archivo (`dev`/`ino` en `vault.rs`), papelera, proceso de la ventana (`ui/action.rs`). Sin esto, las ramas paralelas chocan en los mismos archivos.
2. **En paralelo, después:** canal del CLI en Windows (W5, tubería con nombre), rutas y nombres reservados de Windows (`vault.rs`, `paths.rs`), `update.rs` y `getrandom`, empaquetado en CI (Flatpak, `.deb`, `.exe` instalador y portable) y W4.
3. **La interfaz compartida (opción A, React):** es la parte grande. Sustituye la interfaz GTK también en Linux. El marco se compila aparte con Vite, como CodeMirror (ADR 0028), y va dentro del binario. Rust conserva la lógica (`tabs.rs`, bóveda, frontera de confianza); el marco solo dibuja y envía órdenes. Cada vista de página sigue aislada del marco, y el marco usa su propio proceso web.
   Reparto acordado (2026-10-19). Todas las áreas pasan por `app.rs`; para que las ramas no choquen, la fase 0 deja un módulo vacío por área y las tareas solo rellenan el suyo.
   - **Fase 0, en serie (hecha):** anfitrión en Linux (marco a toda la ventana, páginas en los huecos que anuncia; corregir el fallo de U1 al encoger); protocolo con tipos (`enum` en Rust y tipos en TypeScript) y un módulo por área; frontera (solo el webview del marco da órdenes de marco, con clave y origen propios); Vite con el compilado subido a `assets/`; colores del tema como variables CSS y los iconos en SVG. Se quita la interfaz GTK.
   - **Fase 1, en paralelo, una rama por área (hecha):** A, panel de archivos (`sidebar.rs`); B, pestañas, lados, divisor, zonas para soltar y pantalla completa (`panes.rs`, parte de `app.rs`); C, barra (`bar.rs`); D, buscador (`find.rs`); E, ajustes, actualización y atajos (parte de `sidebar.rs`, `shortcuts.rs`). D y E se cargan al abrirse (`React.lazy`).
   - **Fase 2, en serie:** anfitrión en Windows con winit y lo que diga W4; publicación. La medición frente a GTK en Linux ya está hecha.
4. **ADR a actualizar (con consulta):** 0001 (tao, wry y GTK; winit en Windows; interfaz en HTML) y 0004 (iconos con cairo, pasarían a SVG).

Ideas para recuperar memoria y arranque (sin medir todavía):

- Cargar solo los componentes que se ven (`React.lazy`); decidido por el usuario.
- Pintar el marco en HTML al compilar (prerenderizado), para que aparezca antes de que arranque React.
- Guardar el árbol de la última bóveda y pintarlo al abrir.
- Probar el límite de memoria de los procesos de WebKit (`WebKitMemoryPressureSettings`).
- Preact como sustituto ligero de React, si `lazy` no basta.

## Experimentos diferidos (Mac)

**M1.** Cuando se retome: probar en el ejecutor de macOS (gratis en repositorios públicos) si `127.0.0.2` se puede abrir, el aviso de Gatekeeper sin firma y la compilación para x86_64 y aarch64. Hasta entonces, evitar decisiones que cierren la puerta: no depender de `127.0.0.2`, concentrar las rutas en `paths.rs`, captura tras una interfaz por plataforma.

## Mejoras sueltas que se proponen (no se hacen sin aprobación)

No dependen del resultado de los experimentos y mejoran visor también en Linux:

- `http.rs:92`: sustituir `/dev/urandom` por el crate `getrandom` (seguridad: es el secreto de las URL).
- Validar nombres reservados de Windows en `new_in` y `files.rs`.
- Desactivar la actualización automática si un gestor de paquetes instaló el binario.

## Orden sugerido

1. **Primero, en CI y en paralelo:** L1 y L2 (contenedores), W1, W2 y W3.
2. **G1 con W3.** Si pasa: W4, W5, W6 y U1/U2.
3. **G2 y G3.**
4. **W7** con el usuario.
5. Solo entonces, un plan de construcción nuevo, basado en lo que quede vivo.

## Decisiones del usuario

- **2026-10-16:** sube la rama del worktree y lanza su CI.
- **2026-10-19:**
  - **G1 pasa con winit.** Windows sigue adelante. La ventana de Windows usa winit en lugar de tao, por el bloqueo de W2. Linux sigue con tao y GTK. Hay que actualizar el ADR 0001 (con consulta).
  - **G3: RHEL, Alma y Rocky 9 y Alpine quedan fuera.** Son un nicho muy específico.
  - **Formatos:** en Windows, `.exe` instalador y `.exe` portable. En Linux, Flatpak y `.deb`; AppImage se descarta (registro de descartes).
  - **Linux Mint:** probar en un escritorio X11 real. Si no se consigue, publicar con un aviso de que no está probado en Mint e invitar a colaborar con esa prueba.
  - **La construcción se reparte en tareas paralelas,** con subagentes, cada uno en su rama.
  - **G2: opción A, el marco en HTML, con React + shadcn/ui** (Tailwind, Radix). React y Svelte costaron casi lo mismo (ver «Resultados»); el usuario prefiere el ecosistema de React, que tiene más componentes para un futuro chat con IA. El diseño del prototipo Svelte le gustó más y se puede llevar a React. Acepta el coste medido (+97 a +110 MB frente a visor con GTK) y quiere **cargar solo los componentes que se ven**.
  - **Objetivo futuro:** un chat con IA dentro de visor con la clave o el servidor del usuario (BYOK/BYOS). El marco debe permitirlo. Condiciones ya acordadas: la clave la guarda Rust, nunca el JavaScript del marco, y lo que aporte una página al chat se trata como no confiable (`AGENTS.md`). Componentes de terceros solo con licencia compatible con MIT (AICSS Pro no es público: revisar su licencia antes de usarlo).
  - **Mejorar el aspecto de las «conversaciones» de las notas** se puede hacer ya, aparte del port (`src/page/`).
- **2026-10-19 (tarde):**
  - Dependencias solo de Windows: `windows-sys`, `getrandom` y `sha2`.
  - Paquetes: identificador Flatpak `io.github.ruben_salas20.visor`; Flatpak con acceso a toda la carpeta personal; una copia es «de paquete» por dónde está el ejecutable (`/usr`, salvo `/usr/local`, o `/app`); responsable del `.deb`, el correo de los commits.
  - Los paquetes definitivos esperan a la interfaz nueva; L5 fue solo la prueba de viabilidad.
  - Seguridad: un enlace a una carpeta oculta no se lista ni se usa desde el panel (Linux y Windows); en Windows, la CLI comprueba que la tubería es del mismo usuario, y el panel salta lo que Windows marca oculto o de sistema y los nombres cortos de carpetas ocultas.
  - ADR 0035 ampliado con Windows y paquetes; ADR 0040 nuevo, que amplía el 0001 y sustituye al 0004 cuando el marco reemplace a GTK.

- **2026-10-19 (noche):**
  - W4 y el plan de la interfaz, a la vez. W4 corre en la rama `port-w4` (`spikes/w4-funciones/`).
  - **Sustitución directa:** la rama del port quita la interfaz GTK al empezar, sin interruptor. visor queda incompleto en esta rama hasta acabar la fase 1, y no se fusiona en `main` antes.
  - **El marco compilado se sube al repositorio**, como CodeMirror (ADR 0028): `cargo build` no necesita Node.
  - **Subagentes de la fase 1:** Opus para A y B, Haiku para C, D y E; todos con encargo acotado.
  - **Diseño del marco: D** (`spikes/maquetas-marco/`, `?v=d`). Ajustes sin la versión; botón de atajos (Ctrl+K) en la barra de estado.

- **2026-10-23:** en Windows, dos binarios, como `python.exe` y `pythonw.exe`: `visor.exe` sigue siendo de consola (CLI y MCP) y `visorw.exe`, sin consola, es el que abren el Explorador y los accesos directos. Rama `port-console`.

## Decisiones pendientes del usuario

- **CSP para el origen del marco:** propuesta sin decidir.

## Registro de descartes

| Opción | Estado | Evidencia |
|---|---|---|
| GPUI | Descartada | Sin webviews en Wayland nativo: `build_as_child` solo X11 en wry 0.57; gpui-kit PR 3395; el crate `gpui` oficial es 0.2.2 de oct. 2025 y el ecosistema usa instantáneas semanales [V]. Revisado el 2026-10-19: `gpui` sigue en 0.2.2; `rgpui-webview` 0.5.0 (agosto de 2026, 113 descargas) monta en Linux un `gtk::Fixed` sin ventana y su propio código dice «aún no funciona del todo» [V] |
| Tauri 2 como base | Descartada | Su multiwebview (`add_child`) sigue tras la feature `unstable` [V]. Se puede usar su empaquetador sin migrar [S] |
| Opción D (GTK en Linux + interfaz aparte en Windows) | Descartada | Dos bases de UI: mantenimiento muy alto |
| AppImage | Descartada (usuario, 2026-10-19) | Sin empaquetar WebKit no resuelve nada; empaquetándolo, 70 MB o más [S] |
| Snap | Descartada | Confinamiento parecido a Flatpak, poca adopción fuera de Ubuntu [S] |
| Mac | Aplazada | Sin cuenta de Apple de pago |
| Firma de código en Windows | Descartada por ahora | El usuario no pagará; se acepta SmartScreen. SignPath Foundation es gratis para código abierto y se puede retomar si cambia la decisión [S] |
| RHEL/Alma/Rocky 9, Alpine | Descartadas (usuario, 2026-10-19) | L2: AlmaLinux 9 no trae WebKitGTK 4.1; Alpine usa musl [V] |
| tao en Windows | Descartado (usuario, 2026-10-19) | W2: el bucle se bloquea con varios webviews hijos; winit no [V] |
| Opción A (interfaz en HTML) | Elegida (usuario, 2026-10-19), con React | U1: funciona en Linux, frontera de seguridad intacta, +54 MB el marco y +43 MB React frente a visor |
| Svelte + shadcn-svelte | Descartado (usuario, 2026-10-19) | U1: mismo coste que React (+8 MB y +3 ms); menos componentes para IA |
| Opción B (egui/slint) | Descartada en Wayland (U2, 2026-10-19) | eframe 0.33 y slint 1.18 dan un manejador Wayland y `build_as_child` de wry 0.57 devuelve `UnsupportedWindowHandle` (`webkitgtk/mod.rs:125-153`, solo Xlib). `build_gtk` funciona, pero en una ventana GTK aparte. `spikes/u2-toolkit-rust/` [V] |
| Windows | Viva | W3: 14 de 14; G1 pasa con winit |
| Flatpak | Elegida, a probar | Pendiente de L5 (socket del CLI dentro del sandbox) |
| `.deb` | Elegida (usuario, 2026-10-19) | Usa el WebKitGTK del sistema; L5 lo prueba en Ubuntu 22.04 |
