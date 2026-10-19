# Glosario de visor

La interfaz y la ayuda usan los términos en español; el código, nombres en inglés (entre paréntesis cuando difieren).

## Quién usa visor

- **Persona**: quien lee y estudia en visor. La interfaz y `src/agent/help.txt` dicen «persona».
- **Agente**: programa de IA que controla visor por CLI o MCP. Firma sus respuestas a las anotaciones como `"agente"`.

## Contenido

- **Bóveda** (`vault`): carpeta con páginas. visor la muestra como árbol y la sirve por HTTP, en una dirección del bucle local propia de cada bóveda (127.0.0.1, .3, .4…).
- **Página**: archivo `.html`, `.md` o dibujo.
- **PDF** (`pdf`): `.pdf` de una bóveda. No es página (`vault::is_page`): se abre en el visor completo de PDF.js, servido por visor (ADR 0037) y admite anotaciones de texto, ancladas a página y cita, pero no se edita. Un PDF suelto se abre en el visor de WebKit, sin anotaciones.
- **Nota**: página `.md` que no es dibujo. Se lee renderizada, se edita con Ctrl+E y admite `[[enlaces]]` de Obsidian.
- **Dibujo** (`drawing`): página `.excalidraw.md` (formato del plugin de Excalidraw de Obsidian) o `.excalidraw` (JSON de excalidraw.com). Se lee como lienzo, se edita con Ctrl+E y se guarda en su mismo formato. No es nota: no tiene `[[enlaces]]` ni anotaciones.
- **Página suelta** (`loose`; «archivo suelto»): `.html` fuera de toda bóveda conocida. Se abre como pestaña, sin crear bóveda. Un `.md` fuera de toda bóveda abre su carpeta como bóveda, salvo con `visor open --loose` (el doble clic del gestor de archivos): entonces es suelto: se edita y guarda en su archivo, no se anota, y sus `[[enlaces]]` se buscan solo en su carpeta.
- **Web externa**: página `http` o `https`. No es de confianza.
- **Anotación** (`annotation`): resaltado, comentario o marca sobre un texto, un elemento o un punto. Es una conversación entre persona y agente, guardada en `<bóveda>/.visor/anotaciones/<ruta de la página>.json`.
- **Anclaje** (`anchored`): la anotación sigue encontrando su texto o elemento; si no, queda huérfana (`anchored: false`).
- **Pendiente** (`pending`): la persona espera la respuesta del agente.

## Ventana

- **Ventana** (`App` en `app.rs`; `WinId`): cada ventana de visor, con su bóveda, sus lados y sus pestañas. Un solo proceso las abre todas (ADR 0039). Las órdenes de la CLI van a la **ventana con foco**, la última que usó la persona, salvo con `--window N`.
- **Panel** (`sidebar`; área `files` del marco): árbol lateral de la bóveda.
- **Lado** (`Side` en `tabs.rs`; área `sides` del marco; en la configuración, `panes` y `PaneState`): cada mitad de la vista dividida, con su fila de pestañas y su fila de dirección (atrás, adelante, recargar, la ruta y los botones de anotaciones). Las órdenes de la CLI actúan sobre el lado con el foco.
- **Pestaña** (`Tab`): documento abierto en un lado. La **activa** es la visible de su lado.
- **Pestaña viva** (`Tab.view` presente): tiene webview. Máximo 3 por ventana (`MAX_LIVE`). La **descargada** (sin `view`) se recarga al volver a ella.
- **Barra** (`bar`; área `bar` del marco): la fila de dirección de cada lado. Escribir en ella (Ctrl+L) vuelve en la fase 1 del port (#35).
- **Marco** (`frame`, `marco/`): la interfaz de la ventana, en HTML, en su propio webview. Dibuja y pide; Rust decide.
- **Espacio de trabajo** (`workspace`): pestañas, división y lado con el foco de una bóveda (`workspaces` en la configuración).
- **Posición de lectura** (`position`): línea y altura, o solo altura. Se guarda al ocultar una pestaña y se restaura al recargarla.

## Seguridad y escritura

- **Frontera de confianza**: Rust decide qué puede hacer cada página por la URL del documento cargado en su vista (`vault::access`); ver `AGENTS.md`.
- **Página de la bóveda**: servida desde el origen de su bóveda; puede tener anotaciones y escritura. Las sueltas tienen su propio origen (127.0.0.2) y solo usan consola y `visor.send`.
- **Escritura propia** (`OwnWrite` en `watch.rs`): archivo que visor acaba de escribir. Se reconoce por el hash de su contenido, no por el tiempo, para no tapar los cambios de un agente.

## Términos que debes evitar

- «usuario» en interfaz y ayuda: di «persona». (`AGENTS.md` y los ADR dicen «usuario» para el propietario del proyecto; no mezcles ambos en un texto.)
- «panel» para un lado; «barra lateral» para el panel.
- «activa» para una pestaña con webview: es «viva».
- «comentario» para toda la anotación: es solo el texto de la persona.
- «muestra» o «HTML aislado» para una página suelta.
