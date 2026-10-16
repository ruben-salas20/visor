# Plan de investigación: visor para cualquier persona, en Linux y Windows

Fecha: 2026-10-16. Sale del [informe](informe-multiplataforma.md) y de seis investigaciones en paralelo.

## Cómo se usa este plan

**No es un plan de construcción.** Es una lista de experimentos pequeños, cada uno con una hipótesis, un criterio de éxito y un criterio de fracaso. Hasta que un experimento no pasa, no se escribe código de producción. Cada resultado se anota en el registro de descartes (apartado final), y lo descartado no se vuelve a discutir sin datos nuevos.

Lo investigado hasta ahora fue leyendo código, documentación y páginas de paquetes. **Nada se ha probado en Windows ni en otras distros.** Marcas: [V] verificado leyendo; [S] supuesto, a probar.

Reglas del proyecto que se mantienen:

- Mac queda fuera por ahora (sin cuenta de Apple de pago).
- Se acepta perder rendimiento y peso. Aun así, cada experimento mide lo que pueda con `docs/pruebas.md`.
- La frontera de confianza de `AGENTS.md` no se relaja en ninguna plataforma. Si un experimento muestra que no se puede mantener, esa vía se descarta.
- Los prototipos viven en `spikes/` dentro del worktree, son desechables y no se mezclan con `src/`.

## Lo que ya sabemos [V salvo indicación]

- **Linux.** El binario exige glibc ≥ 2.39 solo por dos símbolos opcionales de Rust. Deja fuera Ubuntu 22.04, Debian 12, Mint 21, RHEL 9 y Leap 15.6. Arranca bajo X11 (XWayland); no se ha probado entero. Según el agente, RHEL 9 no trae WebKitGTK 4.1 y Alpine usa musl [páginas de paquetes; glibc de memoria].
- **Windows.** wry da el webview, pero no `Committed`, captura, fallos de recursos ni pantalla completa. WebView2 los tiene por su API COM. La implementación de wry de `Started` lee `Source()` y no sirve para la frontera de confianza.
- **Interfaz.** Sin GTK en Windows hay que rehacer panel, pestañas, barra y divisores (~4.000 líneas). Candidatas vivas: A (interfaz en HTML) y B (toolkit de Rust).
- **Código.** Hay ataduras que el primer informe no vio (`/dev/urandom` en el secreto de las URL, `is_loose` con `'/'`, `canonicalize` con `\\?\`, nombres reservados de Windows, `LANG`). `std::os::unix` está en 6 archivos de producción, no en 8.

## Puertas de decisión

| Puerta | Pregunta | Se decide con | Si falla |
|---|---|---|---|
| G1 | ¿Es viable Windows con la frontera de confianza intacta? | W2, W3 | Se detiene todo el port a Windows y se replantea |
| G2 | ¿Qué interfaz compartida usamos? | U1, U2 (y W2 repetido con ella) | Se vuelve a evaluar D (dos bases) |
| G3 | ¿Qué alcance real tiene Linux y qué formatos ofrecemos? | L1, L2, L3 | Se acota la lista de distros prometidas |

## Experimentos de Linux

Los hago yo en esta máquina (algunos necesitan contenedores o una máquina virtual).

**L1. ¿Compila y arranca el release en una base antigua?**
- Hipótesis: compilar en Debian 12 (glibc 2.36, WebKitGTK 2.50.6 [V]) da un binario que baja la exigencia de glibc a ~2.36 sin cambiar el código.
- Cómo: contenedor Debian 12, `cargo build --release`, `ldd` y `objdump -T` para ver las versiones exigidas.
- Éxito: compila con la feature `v2_38` de `webkit2gtk` y la exigencia de glibc baja.
- Fracaso: no compila (API o versión de crates) o el binario sigue exigiendo glibc ≥ 2.39. Entonces se prueba Ubuntu 22.04 y, si falla también, se acota el alcance.
- Necesita: un servicio de contenedores. Docker está instalado pero su servicio no responde; puede arrancarlo el usuario con `! sudo systemctl start docker`. La alternativa es hacerlo en GitHub Actions.

**L2. ¿En qué distros arranca de verdad?**
- Hipótesis: con el binario de L1, `visor status` responde bajo `xvfb` en Ubuntu 22.04 y 24.04, Debian 12 y 13, Fedora, Arch y openSUSE; y falla en RHEL/Alma 9 y Alpine.
- Cómo: contenedor por distro, instalar WebKitGTK 4.1 y `xvfb`, lanzar y consultar el estado.
- Éxito: tabla con cada distro marcada «funciona», «falla (motivo)» o «no probada».
- Fracaso: resultados que contradicen las tablas de paquetes. Se registra igual: el dato es el producto.

**L3. ¿Funciona entero bajo X11?**
- Hipótesis: panel, arrastre, menús emergentes, escala y teclado funcionan igual que en Wayland.
- Cómo: lista de comprobación manual bajo XWayland y, si hay una sesión Xorg o una máquina virtual disponible, bajo X11 real. Con escala 1 y 1,5.
- Éxito: sin diferencias visibles. Se cambia el aviso del README.
- Fracaso: fallos concretos de GTK3 en X11. Se anotan; el aviso pasa a «Wayland probado, X11 experimental».

**L4. ¿Compila en aarch64?**
- Cómo: ejecutor ARM nativo de GitHub Actions (`ubuntu-22.04-arm`).
- Éxito: compila y arranca. Fracaso: se anota qué dependencia falla.

**L5. Formatos de paquete: ¿cuáles encajan?**
- `.deb` con `cargo-deb`: instalarlo en un contenedor Ubuntu 22.04 limpio y comprobar que resuelve las dependencias.
- Flatpak: ¿puede el CLI del host hablar con la app dentro del sandbox? Prueba mínima de socket en `xdg-run` y de acceso a una carpeta arbitraria. Si el socket no se alcanza sin permisos amplios, Flatpak se descarta por ahora.
- `update.rs` bajo un paquete gestionado: comprobar que se desactiva.

## Experimentos de Windows

**Los ejecuta el usuario en su equipo Windows.** Yo preparo cada prototipo en `spikes/` como proyecto Rust independiente, con instrucciones de una línea (`cargo run`) y un resultado que se pega aquí. No puedo ejecutar Windows desde esta máquina.

**W1. ¿Cuánto del código compila ya en Windows?**
- Hipótesis: `vault/`, `agent/`, `http.rs`, `session.rs`, `tabs.rs` y `watch.rs` compilan en Windows con unos pocos cambios, dejando la interfaz fuera.
- Cómo: `cargo check` en Windows sobre una copia con `ui/` desactivada. Se cuentan los errores por categoría.
- Éxito: errores acotados y todos de la lista conocida. Fracaso: hallazgos nuevos grandes (se añaden al inventario).

**W2. wry + WebView2: ventana con varios webviews hijos.**
- Hipótesis: con un entorno compartido (`with_environment`), 5-10 webviews hijos cuestan ≤ ~60-65 MB cada uno y se colocan con `set_bounds`.
- Cómo: prototipo con `tao` + `wry`, un servidor HTTP local y 1, 5 y 10 webviews. Medir procesos y memoria con el Administrador de tareas o un script.
- Éxito: el coste por webview es parecido al de WebKitGTK y no hay fallos de foco ni de redimensionado. Fracaso: coste muy superior; entonces cuentan más las pestañas ocultas suspendidas (`TrySuspendAsync`) o un solo webview.

**W3. La frontera de confianza: `Committed` por `NavigationId`.** (decide G1)
- Hipótesis: se puede reproducir `Committed` con `NavigationStarting` + `ContentLoading` unidos por `NavigationId`, sin leer `Source()`.
- Cómo: con `webview2-com` sobre `WebViewExtWindows::webview()`. Páginas de prueba que provoquen: redirección HTTP, `history.pushState`, cambio de fragmento, error de red, navegación solapada, iframe externo y carga pendiente.
- Éxito: la URL registrada nunca cambia por `pushState` ni por fragmento, y es la del documento realmente cargado en todos los casos.
- Fracaso: algún caso en el que la URL registrada no es la del documento. **Se detiene el port a Windows**: sin esta pieza no se puede cumplir `AGENTS.md`.

**W4. Funciones que wry no da.**
- Captura (`CapturePreview`), botones laterales del ratón (por script), pantalla completa, fallos de recursos de subframes, hoja de estilo por script de inicio, zoom y foco.
- Cada una: funciona / funciona con límites / no funciona. Las que no funcionen se anotan como funciones que Windows no tendría.

**W5. IPC del CLI.**
- Tubería con nombre con ACL al usuario actual e instancia única, frente a TCP en localhost con token. Criterio: otro usuario del mismo equipo no puede conectar; el CLI y el MCP hablan igual que con el socket Unix.

**W6. Runtime ausente.**
- Qué ocurre en un Windows 10 sin WebView2: flujo del bootstrapper y mensaje de error sin red. Se necesita una máquina virtual limpia.

**W7. Firma y SmartScreen.**
- Comprobar si visor cumple las condiciones de SignPath Foundation (licencia OSI, repositorio público) y qué ve un usuario con un `.exe` sin firmar. Coste: cero. Plazo: lento, conviene empezar pronto.

## Experimentos de interfaz

**U1. Prototipo A: panel, pestañas y barra en HTML.** (en Linux, 4 h)
- Cuadro: lista de pestañas, árbol de ~200 nodos, barra, y 2 webviews de página en `gtk::Fixed`. Recibe los efectos de `tabs.rs` y envía eventos por IPC.
- Mide: arranque, PSS del árbol de procesos, CPU en reposo y peso (`tools/medir/medir.py`).
- Prueba: arrastre del árbol a una página; lectura con Orca; y que **una página no pueda alcanzar los mensajes de la interfaz** (frontera de confianza).
- Fracaso: si el arrastre sobre webviews hijos no se puede resolver o la frontera se rompe, A se descarta.

**U2. Prototipo B: toolkit de Rust (egui o slint) con webviews de wry.** (1 h de corte)
- Primero: ¿arranca en Wayland con `build_gtk` dentro de la ventana? Si no arranca, se descarta en una hora.
- Si arranca: mismas medidas y pruebas que U1.

**U3. GPUI (opcional, solo para refutar).**
- Ventana GPUI con `gpui-webview` y 2 webviews en Hyprland con escala 1,5. Fuera del camino crítico: el estado actual es «descartado» (ver registro).

**Criterio para G2:** se elige A salvo que B ahorre más de ~50 MB y funcione en Wayland con arrastre y lector de pantalla sin parches. Se repite W2 con la interfaz elegida.

## Experimentos diferidos (Mac)

**M1.** Cuando se retome: probar en el ejecutor de macOS de GitHub Actions si `127.0.0.2` se puede abrir, el flujo de aviso de Gatekeeper sin firma y la compilación para x86_64 y aarch64. Hasta entonces, evitar decisiones que cierren la puerta: no depender de `127.0.0.2`, concentrar las rutas en `paths.rs`, captura tras una interfaz por plataforma.

## Mejoras sueltas que propongo (no se hacen sin tu aprobación)

No dependen del resultado de los experimentos y mejoran visor también en Linux:

- `http.rs:92`: sustituir `/dev/urandom` por el crate `getrandom` (seguridad: es el secreto de las URL).
- Validar nombres reservados de Windows en `new_in` y `files.rs`, aunque Windows aún no exista.
- Desactivar la actualización automática si un gestor de paquetes instaló el binario.

## Orden sugerido

1. En paralelo: L1 y L2 (yo, con contenedores) y W1, W2, W3 (tú, con los prototipos que preparo).
2. G1 con W3. Si pasa, W4, W5, W6, W7 y U1/U2.
3. G2 y G3.
4. Solo entonces, un plan de construcción nuevo, basado en lo que quede vivo.

## Registro de descartes

| Opción | Estado | Evidencia |
|---|---|---|
| GPUI | Descartada | Sin webviews en Wayland nativo: `build_as_child` solo X11 en wry 0.57; gpui-kit PR 3395; el crate `gpui` oficial es 0.2.2 de oct. 2025 y el ecosistema usa instantáneas semanales [V] |
| Tauri 2 como base | Descartada | Su multiwebview (`add_child`) sigue tras la feature `unstable` [V]. Se puede usar su empaquetador sin migrar [S] |
| Opción D (GTK en Linux + interfaz aparte en Windows) | Descartada salvo que G2 falle | Dos bases de UI: mantenimiento muy alto |
| AppImage | Descartada por ahora | Sin empaquetar WebKit no resuelve nada; empaquetándolo, 70 MB o más [S] |
| Snap | Descartada | Confinamiento parecido a Flatpak, poca adopción fuera de Ubuntu [S] |
| Mac | Aplazada | Sin cuenta de Apple de pago |
| RHEL/Alma/Rocky 9, Alpine | Sin solución prevista | RHEL 9 no trae WebKitGTK 4.1; Alpine usa musl [páginas de paquetes] |
| Opción A (interfaz en HTML) | Viva | Pendiente de U1 |
| Opción B (egui/slint) | Viva, poco probable | Sin ejemplos con wry en Wayland [V]; pendiente de U2 |
| Windows | Viva, condicionada a G1 | Pendiente de W3 |
| Flatpak | Viva, dudosa | Pendiente de L5 |
