# Menús del marco encima de las páginas

Prototipo del issue 35. Busca cómo dibujar los menús del marco encima de una página viva, en Linux (WebKitGTK, Wayland) y en Windows (WebView2), sin tocar la frontera de confianza.

## Resultado

**La vía 1 funciona en los dos sistemas.** El webview del marco pasa a estar encima de las páginas. Su fondo es transparente en el hueco de cada página. Su entrada se limita a una zona: la ventana, menos los huecos, más los menús abiertos.

- **Linux:** la zona es la forma de entrada de GDK (`input_shape_combine_region`, la región de la ventana GDK que acepta clics). Pasa 11 de 11 pasos en 5 ejecuciones, a escala 1, 1,25 y 1,5.
- **Windows:** la zona es la región de ventana (`SetWindowRgn`) del webview del marco. Recorta a la vez lo que se dibuja y dónde llegan los clics. Pasa 11 de 11 pasos en 3 de 3 ejecuciones (CI 39289889833).

No hace falta ningún webview más. El menú vive en el DOM del marco, como ahora.

## Cómo funciona

1. El marco anuncia sus huecos (como hoy) y, además, la caja de cada menú abierto.
2. Rust calcula la zona: ventana − huecos + menús. La aplica al webview del marco.
3. Un clic fuera del menú, en la página, mueve el foco a la página. El marco recibe `blur` y cierra el menú.
4. Mientras se arrastra algo desde el marco, la zona es la ventana entera. Así el marco recibe la suelta y la página no ve el arrastre.

En Windows la región se amplía 24 px alrededor del menú para que se vea la sombra. Con un fondo transparente, WebView2 mezcla bien dos vistas hermanas: la sombra cae sobre la página (`#266729` sobre el verde `#2e7d32`, igual que en Linux).

## Pasos de la prueba

Cada paso usa entrada real. En Linux, un puntero virtual de wlroots (`src/bin/puntero.rs`) y `wtype`. En Windows, `SendInput`. Los colores se leen de la pantalla ya compuesta (`grim` y `GetPixel`).

| Paso | Qué comprueba | Linux | Windows |
|---|---|---|---|
| 1 | Con el menú cerrado, la página se ve por el hueco | Sí | Sí |
| 2 | El menú se dibuja encima de la página | Sí | Sí |
| 2b | La sombra del menú cae sobre la página | Sí | Sí, con la región ampliada |
| 3 | Un clic en un elemento del menú, sobre la página, llega al marco y no a la página | Sí | Sí |
| 4 | Con el menú cerrado, el mismo punto llega a la página | Sí | Sí |
| 5 | La rueda desplaza la página | Sí | Sí |
| 6 | El teclado escribe en un campo de la página | Sí | Sí |
| 7 | Un clic en la página cierra el menú abierto | Sí | Sí |
| 8 | Un arrastre desde el panel lateral se suelta en el marco, no en la página | Sí | Sí |
| 9 | La página no alcanza al marco: su orden falsa queda en su propio manejador; `top`, `parent`, `opener` y `__marco` no existen | Sí | Sí |
| 10 | Tras redimensionar, la zona sigue a los huecos | Sí | Sí |

Controles:

- **Linux sin forma de entrada** (`MENUS_SIN_FORMA=1`): fallan los pasos 4, 5, 6, 7 y 10. La forma es lo que reparte la entrada.
- **Windows sin región** (`--modo transparente`, CI 39288505817): se ve todo bien, sombra incluida, pero todos los clics van al marco. Fallan 4, 5, 6, 7 y 10.
- **Windows sin capturar el arrastre** (CI 39288505817): la suelta llegó a la página. Por eso el paso 8 amplía la zona durante el arrastre.
- **Windows con fondo opaco** (`--modo opaco`): todo funciona menos la sombra, que la región recorta.

Captura de Linux con el menú abierto: `capturas/linux-menu-abierto.png`.

## Las cuatro vías

| Vía | Linux | Windows | Frontera | Coste | Veredicto |
|---|---|---|---|---|---|
| 1. Marco encima, huecos transparentes, zona de entrada | Funciona (prototipo) | Funciona (prototipo, CI) | Intacta: cada vista sigue con su manejador y su `Committed` | Ningún proceso ni webview más; una región por cambio de huecos o menús | **Elegida** |
| 2. Tercer webview transparente solo para emergentes | Posible (`related-view` comparte proceso) | Posible: otro webview, unos 25 MB (W2) | Intacta si tiene su origen y clave | Un webview más y estado del menú repartido entre dos documentos | Peor que la 1 sin ventaja |
| 3. Ventanas emergentes nativas | GTK `popup` + `xdg_popup`: se colocan respecto a la ventana, no en coordenadas absolutas | `WS_POPUP` con un webview (unos 25 MB) o menús nativos (`muda`) | Intacta | Webview por emergente o menús que no siguen el diseño D | Descartada: aspecto o coste |
| 4. Páginas en iframes dentro del marco | — | — | **Rota** | — | Descartada |

Motivos de la vía 4, en concreto:

- **Sin aislamiento de proceso.** WebKitGTK no separa los iframes de otro origen en otro proceso. En julio de 2025 Igalia aún lo estudiaba para GTK y WPE; no hay versión que lo traiga. Un fallo explotable en una página alcanzaría el proceso del marco.
- **Sin `Committed` por iframe.** WebKitGTK solo da a Rust los eventos de carga del marco principal (`load-changed`). Sin extensión web, Rust no sabe qué documento hay en cada iframe, y `vault::access` decide por esa URL.
- **Sin identidad en el IPC.** El manejador de mensajes es uno por vista. Rust no sabría de qué iframe viene cada mensaje: el marco tendría que reenviarlos y Rust confiaría en lo que dice el marco.
- **Webs externas.** Muchas prohíben que las metan en un iframe (`X-Frame-Options`, `frame-ancestors`).
- **Agentes.** `visor eval` corre en el documento principal y `visor screenshot` captura la vista entera, no un iframe.

WebView2 en modo composición (`ICoreWebView2CompositionController`) no hace falta: el modo ventana que usa wry 0.57 ya mezcla bien vistas hermanas transparentes. Además, wry no lo ofrece; habría que crear el control a mano y reenviar ratón, cursor, arrastre y accesibilidad.

## Qué haría falta para llevarlo a `src/`

1. `ui/host.rs`: invertir el orden del `gtk::Overlay`. Las páginas pasan a la capa de abajo y el marco a la de arriba, con `with_transparent(true)`. Aplicar la forma a cada ventana GDK entre la vista y la ventana principal: GtkOverlay da a su hijo una ventana propia, y sin forma esa ventana se queda los clics.
2. `ui/protocol.rs` y `marco/src/holes.ts`: el marco anuncia, junto a los huecos, las cajas de sus emergentes y si está arrastrando. Los componentes de shadcn/ui que abren menús en un portal tienen que avisar al abrir y al cerrar.
3. El marco cierra sus menús con `blur`.
4. Windows (anfitrión de winit): `SetWindowRgn` sobre el `hwnd()` del marco, en píxeles físicos, y el marco por encima de las páginas (`SetWindowPos` con `HWND_TOP`).

## Qué queda sin probar

- Clic, arrastre y rueda con un ratón físico en Hyprland y en un Windows de escritorio.
- Escalas distintas de 1,0 en Windows (en Linux, 1,25 y 1,5 pasan).
- Coste en CPU en Linux: GTK vuelve a pintar la capa del marco encima de cada repintado de la página (vídeo, desplazamiento). En Windows la mezcla la hace DWM.
- Lector de pantalla (Orca y Narrador) con el marco encima.
- Arrastrar archivos desde fuera de visor a una página.
- Varias páginas y vista dividida (varios huecos): la zona ya admite una lista de huecos, pero solo se probó uno.
- Rendimiento con `docs/pruebas.md`: no se midió.

## Cómo ejecutar

- Linux: `cargo build --release` y después `./pruebas/ejecutar.sh`. Abre un sway sin pantalla propio, nunca el escritorio. Variables: `ESCALA=1.5`, `MENUS_SIN_FORMA=1`, `MENUS_CAPTURAS=<carpeta>`.
- Windows: flujo `.github/workflows/windows-menus.yml`, rama `port-menus`.
