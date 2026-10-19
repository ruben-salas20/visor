# W4 en Windows: siete funciones de WebView2 que wry no da

Prototipo del issue 35. Comprueba siete funciones que visor usa en Linux y que wry 0.57 no ofrece en Windows. Cada una se prueba con la API COM de WebView2 (`webview2-com` 0.39), a la que se llega con `WebViewExtWindows::webview()` y `controller()`.

Montaje: una ventana winit y webviews hijos de wry (`build_as_child`). Un servidor local en 127.0.0.1 sirve las páginas de prueba, que van dentro del binario. El mismo servidor, pedido como `localhost`, hace de otro sitio: así hay un iframe de otro origen, que Chromium pone en otro proceso.

Solo se ha ejecutado en GitHub Actions (`.github/workflows/windows-w4.yml`, runner `windows-2025`, pantalla de 1024×768 con escala 1,0, WebView2 Runtime 154.0.4258.62).

## Uso

```
w4-funciones --auto <salida.json> --prueba captura|pantalla|fallos|estilo|zoom|foco|lateral
```

Termina solo. Código de salida: 0 si se cumple el criterio, 1 si no se cumple o hay un error, 3 si se agota el tiempo (90 s). El JSON trae `ok`, `resultado`, `motivo` y la evidencia. La captura se guarda junto al JSON, con extensión `.png`.

## Resultados (ejecuciones 39273048404 y 39273561199)

Las dos ejecuciones dan lo mismo en las siete pruebas.

| Función | Resultado | Evidencia |
|---|---|---|
| 1. Captura | Funciona | `ICoreWebView2::CapturePreview` en PNG sobre un `IStream` de memoria: 34–35 ms, 2,7 kB. El PNG mide 800×600, igual que la vista. Los píxeles de las dos mitades salen exactos: (10, 200, 90) y (200, 30, 60). |
| 2. Pantalla completa | Funciona | `requestFullscreen()` lanzado con `ExecuteScript` entra **sin gesto del usuario**. Rust recibe `ContainsFullScreenElementChanged` al entrar (`true`) y al salir con `exitFullscreen()` (`false`). La ventana winit pasa a 1024×768 sin bordes y vuelve a 800×600. WebView2 no agranda la vista solo: Rust tiene que ponerle los `bounds` de la ventana. |
| 3. Fallos de recursos | Funciona con límites | Hacen falta dos fuentes. `WebResourceResponseReceived` da el estado HTTP de todos los marcos, también del iframe de otro sitio, pero no ve los errores de red ni dice de qué marco viene la respuesta. El protocolo DevTools (`Network.enable`, `Network.loadingFailed` y `Network.responseReceived`) ve el error de red (`net::ERR_CONNECTION_REFUSED` del script) y dice el tipo y el marco, pero no ve el iframe de otro sitio. Los tres fallos pedidos llegan con su URL y su motivo. El documento principal no cuenta: una página principal 404 llega como `Document` del marco principal y el filtro la descarta. Chromium pide `/favicon.ico`, que da un 404 de más: hay que filtrarlo. |
| 4. Hoja de estilo inyectada | Funciona con límites | `AddScriptToExecuteOnDocumentCreated` con un `<style>` llega al documento principal y a los dos iframes, también al de otro sitio. El cambio de tema en caliente funciona sin recargar: se cambia el script para los documentos nuevos y se ejecuta el nuevo con `ExecuteScript` en la página y con `ICoreWebView2Frame2::ExecuteScript` en cada marco (los marcos se recogen con `FrameCreated`). Tras recargar se mantiene el tema nuevo. **Límite**: el estilo entra al nivel de la página, no al de usuario. Una regla de la página con `!important` gana al tema aunque el tema también use `!important`. En WebKit, el `!important` del nivel de usuario gana. |
| 5. Zoom | Funciona | `wry::WebView::zoom(1.5)` basta: llama a `SetZoomFactor`. El ancho pasa de 800 a 533 px CSS y `devicePixelRatio` de 1 a 1,5. El factor se conserva al navegar a otra página del mismo origen y también a otro origen: es de la vista, no del sitio. |
| 6. Foco | Funciona | Con dos vistas, `wry::WebView::focus()` (que llama a `MoveFocus`) en B produce `GotFocus` de B, y `document.hasFocus()` es `true` en B y `false` en A. Al revés, igual. Rust se entera con `add_GotFocus`/`add_LostFocus` del `ICoreWebView2Controller`. |
| 7. Botones laterales | Funciona | `SendInput` con XBUTTON1 sí llega en el runner. Sin hacer nada, WebView2 vuelve atrás solo, y la página ve el botón 3. Hay dos formas de impedirlo. Por script: `preventDefault()` en los eventos del botón 3. La prueba lo llama en todos (`pointerdown`, `mousedown`, `pointerup`, `mouseup`, `auxclick`), así que no aísla cuál basta; al cancelar `pointerdown`, `mousedown` y `mouseup` ya no llegan. En Rust: cancelar en `NavigationStarting` las navegaciones de tipo `BACK_OR_FORWARD` (`ICoreWebView2NavigationStartingEventArgs3::NavigationKind`). La forma de Rust no depende de la página. |

## Qué cambia respecto a Linux

- **Fallos de recursos**: en WebKit una sola señal cubre todo. En WebView2 hay que juntar dos fuentes, y quedan sin cubrir los errores de red dentro de un iframe de otro sitio. Para verlos, el protocolo DevTools tendría que engancharse a cada marco de otro proceso (`Target.setAutoAttach`), que no se probó.
- **Tema**: no hay nivel de usuario. Una página que fuerce colores con `!important` gana al tema.
- **Botones laterales**: cancelar en `NavigationStarting` también cancelaría el «atrás» que pida la propia app con `GoBack`. Hace falta una marca que deje pasar los de visor.
- **Pantalla completa**: la página puede entrar sin gesto del usuario cuando el script lo lanza el anfitrión.

## Qué no se probó

- Un clic real del ratón ni el teclado. El foco por clic queda para la prueba manual (W7).
- Un ratón con botones laterales de verdad. `SendInput` simula el mismo mensaje del sistema, pero conviene confirmarlo en W7.
- `requestFullscreen()` lanzado por la página tras un clic real, que es el caso normal de un vídeo.
- Errores de red dentro de un iframe de otro sitio (ver arriba).
- Escalas de pantalla distintas de 1,0 y Windows de escritorio (10 u 11). El runner es Windows Server 2025.
- El coste en memoria y CPU de tener activo el dominio `Network` del protocolo DevTools.

## Nota para el port

Las pruebas esperan dentro de `resumed` despachando mensajes de Win32 con `PeekMessageW`. Mientras un callback de winit está en marcha, winit guarda `WM_PAINT` en cola sin validar la ventana, y Windows lo vuelve a generar sin fin. Un bucle `while PeekMessageW(...)` sin tope se cuelga (ejecuciones 39270309883 y 39272518136). Con un tope de mensajes por vuelta, funciona. visor no debería esperar así: mejor responder a los eventos desde el bucle de winit.
