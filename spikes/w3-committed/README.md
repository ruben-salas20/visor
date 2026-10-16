# W3: URL comprometida en WebView2

Experimento del issue 35. Pregunta: ¿se puede determinar en WebView2 la URL del documento
comprometido (lo que WebKitGTK da en `LoadEvent::Committed`) uniendo `NavigationStarting` y
`ContentLoading` por `NavigationId`, sin usar `Source()` para decidir?

## Hipótesis

Cada navegación tiene un `NavigationId`. Se guarda la URI de `NavigationStarting` por id (una
redirección sobrescribe la URI de su id) y, al llegar `ContentLoading` con ese id, esa es la
URL comprometida. Si `ContentLoading` llega con un id sin `NavigationStarting` previo, no se
compromete ninguna URL (se falla cerrado).

## Casos

| Caso | Qué prueba | Esperado |
|---|---|---|
| `redireccion` | 302 de `/redir` a `/b` | comprometida `/b` |
| `pushstate` | `history.pushState` tras la carga | comprometida `/push`, nunca `/falsa` |
| `fragmento` | cambio de `#hash` | comprometida `/frag` |
| `iframe` | marco hijo en otro puerto que navega solo | comprometida `/iframe`, nunca `/hijo` |
| `solapada` | `/lenta` (5 s) cancelada por `/a` | comprometida `/a`, nunca `/lenta` |
| `error_red` | puerto sin servidor | comprometida la URL, con error |

Cada caso espera su tiempo completo para que lleguen eventos tardíos. El límite global es de 90 s.

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
- No prueba descargas, popups ni `window.open`.
- Los resultados dependen de la versión de WebView2 que reporte el workflow.
