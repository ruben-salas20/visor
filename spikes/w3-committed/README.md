# W3: URL comprometida en WebView2

Experimento del issue 35. Pregunta: ¿se puede determinar en WebView2 la URL del documento
comprometido (lo que WebKitGTK da en `LoadEvent::Committed`) uniendo `NavigationStarting` y
`ContentLoading` por `NavigationId`, sin usar `Source()` para decidir?

## Hipótesis

Cada navegación tiene un `NavigationId`. Se guarda la URI de `NavigationStarting` por id (una
redirección sobrescribe la URI de su id) y, al llegar `ContentLoading` con ese id, esa es la
URL comprometida. Si `ContentLoading` llega con un id sin `NavigationStarting` previo, no se
compromete ninguna URL (se falla cerrado).

`Source()` se lee en dos momentos (al cargar y al final de cada caso), solo para el registro.
En `pushstate` la lectura final debe ser `/falsa`; si no lo es, el caso no ejercita su riesgo
y falla.

## Estado compartido entre casos

Los casos comparten una sola sesión de WebView2: no se reinicia el estado entre ellos. Cada
caso empieza navegando a su punto de partida, y los que dependen de la página anterior la
usan tal cual la dejó el caso previo. Cada caso mide solo los eventos y el historial que
llegan durante él.

## Casos

Comprometida esperada entre paréntesis. `Source al cargar` y `Source al final` se registran
en la tabla y en el JSON.

| Caso | Qué prueba | Esperado |
|---|---|---|
| `redireccion` | 302 de `/redir` a `/b` | `/b` |
| `pushstate` | `history.pushState` tras la carga; `Source()` final debe ser `/falsa` | `/push`, nunca `/falsa` |
| `fragmento` | cambio de `#hash` | `/frag` |
| `iframe` | marco hijo en otro puerto que navega solo | `/iframe`, nunca `/hijo` ni `/hijo2` |
| `solapada` | `/lenta` (5 s) cancelada por `/a` | `/a`, nunca `/lenta` |
| `error_red` | puerto sin servidor | la URL, con error |
| `atras_entre_documentos` | `/a`, `/b`, `GoBack()`; muestra a 1200 ms en `/b` | `/a` |
| `adelante` | lo anterior y luego `GoForward()`; muestra a 2000 ms en `/a` | `/b` |
| `atras_a_pushstate` | `/b`, luego `/push` (pushState a `/falsa`), luego `GoBack()` | `/b`, nunca `/falsa`: Chromium salta las entradas de pushState sin gesto del usuario |
| `script_navega` | `/script` hace `location.href = '/b'` tras cargar | `/b` |
| `recarga` | `/a` y `Reload()`; exige al menos 2 `ContentLoading` | `/a` |
| `sin_contenido` | desde `/a`, `/vacio` responde 204 | `/a` |
| `descarga` | desde `/a`, `/descarga` con `Content-Disposition: attachment`; la descarga se cancela | `/a` |
| `pendiente` | desde `/a`, `/lenta` sin cancelar; muestra a 2000 ms | `/a` a 2000 ms, `/lenta` al final |

Cada caso espera su tiempo completo para que lleguen eventos tardíos. La suma de esperas
mínimas es de unos 39 s; el límite global es de 90 s.

Las comprobaciones de `muestra` (a los N ms, la URL comprometida debe ser la indicada) son
parte del veredicto. `descarga` usa `ICoreWebView2DownloadStartingEventArgs::SetCancel(true)`; si la
cancelación falla, el registro lo muestra como `cancelada=false` y el caso puede fallar.

## Uso

En Windows, con WebView2 Runtime instalado:

    cargo run --release -- salida

Escribe `salida/w3-resultados.json` y `salida/w3-resultados.md`. Sale con código 1 si algún caso
falla. El workflow `.github/workflows/windows-committed.yml` lo ejecuta en `windows-2025`.

En Linux solo compila el stub (`cargo check`). Para la comprobación cruzada:
`cargo check --target x86_64-pc-windows-msvc`.

## Lo que no cubre

- El runner de CI puede no tener sesión gráfica interactiva; si WebView2 no llega a navegar, el resultado no dice nada de la hipótesis.
- No cubre `HistoryChanged` ni `SourceChanged` más allá de registrarlos.
- No prueba navegaciones de marcos hijos con `ContentLoading`: se supone que no llegan.
- No prueba popups ni `window.open`.
- No sabe si `GoBack` desde la caché de páginas (bfcache) dispara `ContentLoading`: el caso `atras_entre_documentos` lo mostrará, no lo supone.
- No sabe si `Reload()` dispara `NavigationStarting`: se registrará tal cual llegue.
- Los resultados dependen de la versión de WebView2 que reporte el workflow.
