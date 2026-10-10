# Glosario de visor

La interfaz y la ayuda usan los términos en español; el código, nombres en inglés (entre paréntesis cuando difieren).

## Quién usa visor

- **Persona**: quien lee y estudia en visor. La interfaz y `src/agent/help.txt` dicen «persona».
- **Agente**: programa de IA que controla visor por CLI o MCP. Firma sus respuestas a las anotaciones como `"agente"`.

## Contenido

- **Bóveda** (`vault`): carpeta con páginas. visor la muestra como árbol y la sirve por HTTP desde 127.0.0.1.
- **Página**: archivo `.html` o `.md`.
- **Nota**: página `.md`. Se lee renderizada, se edita con Ctrl+E y admite `[[enlaces]]` de Obsidian.
- **Página suelta** (`loose`; «archivo suelto»): `.html` fuera de toda bóveda conocida. Se abre como pestaña, sin crear bóveda. Un `.md` suelto no existe como tal: abre su carpeta como bóveda.
- **Web externa**: página `http` o `https`. No es de confianza.
- **Anotación** (`annotation`): resaltado, comentario o marca sobre un texto, un elemento o un punto. Es una conversación entre persona y agente, guardada en `<bóveda>/.visor/anotaciones/<ruta de la página>.json`.
- **Anclaje** (`anchored`): la anotación sigue encontrando su texto o elemento; si no, queda huérfana (`anchored: false`).
- **Pendiente** (`pending`): la persona espera la respuesta del agente.

## Ventana

- **Panel** (`sidebar`, `sidebar.rs`): árbol lateral de la bóveda, en GTK nativo.
- **Lado** (`Side` en `tabs.rs`; widgets `Pane` en `panes.rs`; en la configuración, `panes` y `PaneState`): cada mitad de la vista dividida, con su fila de pestañas, sus botones de anotaciones y su barra. Las órdenes de la CLI actúan sobre el lado con el foco.
- **Pestaña** (`Tab`): documento abierto en un lado. La **activa** es la visible de su lado.
- **Pestaña viva** (`Tab.view` presente): tiene webview. Máximo 3 en toda la ventana (`MAX_LIVE`). La **descargada** (sin `view`) se recarga al volver a ella.
- **Barra** (`bar`, `bar.rs`): barra de direcciones; empieza oculta (`bar_shown`).
- **Espacio de trabajo** (`workspace`): pestañas, división y lado con el foco de una bóveda (`workspaces` en la configuración).
- **Posición de lectura** (`position`): línea y altura, o solo altura. Se guarda al ocultar una pestaña y se restaura al recargarla.

## Seguridad y escritura

- **Frontera de confianza**: Rust decide qué puede hacer cada página por su URL (`vault::access`); ver `AGENTS.md`.
- **Página de la bóveda**: servida desde la bóveda; puede tener anotaciones y escritura. Las sueltas comparten el prefijo de URL, pero solo usan consola y `visor.send`.
- **Escritura propia** (`OwnWrite` en `watch.rs`): archivo que visor acaba de escribir. Se reconoce por el hash de su contenido, no por el tiempo, para no tapar los cambios de un agente.

## Términos que debes evitar

- «usuario» en interfaz y ayuda: di «persona». (`AGENTS.md` y los ADR dicen «usuario» para el propietario del proyecto; no mezcles ambos en un texto.)
- «panel» para un lado; «barra lateral» para el panel.
- «activa» para una pestaña con webview: es «viva».
- «comentario» para toda la anotación: es solo el texto de la persona.
- «muestra» o «HTML aislado» para una página suelta.
