# Glosario de visor

Los términos de la interfaz y de la ayuda van en español. El código usa nombres en inglés. Cuando difieren, se indica entre paréntesis.

## Quién usa visor

- **Persona**: quien lee y estudia en visor. La ayuda (`src/agent/help.txt`) y la interfaz dicen «persona».
- **Agente**: programa de IA que controla visor por la CLI o por MCP. Firma sus respuestas a las anotaciones como `"agente"`.

## Contenido

- **Bóveda** (`vault`): carpeta con páginas. visor la muestra como árbol y la sirve por HTTP desde 127.0.0.1.
- **Página**: archivo `.html` o `.md` que visor muestra.
- **Nota**: página `.md`. Se lee renderizada y se edita con Ctrl+E. Admite `[[enlaces]]` al estilo de Obsidian.
- **Página suelta** (`loose`; también «archivo suelto»): `.html` fuera de toda bóveda conocida. Se abre como pestaña, sin crear bóveda. Una nota `.md` suelta no existe como tal: abre su carpeta como bóveda.
- **Web externa**: página `http` o `https`. No es de confianza (ver «Frontera de confianza»).
- **Anotación** (`annotation`): resaltado, comentario o marca sobre un texto, un elemento o un punto de una página. Es una conversación entre persona y agente. Se guarda en `<bóveda>/.visor/anotaciones/<ruta de la página>.json`.
- **Anclaje** (`anchored`): la anotación sigue encontrando su texto o su elemento. Si no, queda huérfana (`anchored: false`).
- **Pendiente** (`pending`): la persona espera la respuesta del agente.

## Ventana

- **Panel** (`sidebar`, `sidebar.rs`): árbol lateral de la bóveda, en GTK nativo.
- **Lado** (`pane`; modelo en `tabs.rs`, widgets en `panes.rs`): cada mitad de la vista dividida. Tiene su fila de pestañas, sus botones de anotaciones y su barra. Las órdenes de la CLI actúan sobre el lado con el foco.
- **Pestaña** (`Tab`): un documento abierto en un lado.
- **Pestaña viva** (con `view` en `Tab`, `tabs.rs`): pestaña con su webview cargado. Solo puede haber 3 en toda la ventana (`MAX_LIVE`).
- **Pestaña descargada** (sin `view`; en inglés, «unload»): pestaña sin webview. Se recarga al volver a ella.
- **Barra** (`bar`, `bar.rs`): barra de direcciones. Empieza oculta (`bar_shown`).
- **Espacio de trabajo** (`workspace`): pestañas, división y lado con el foco de una bóveda. Se guarda en `workspaces` de la configuración.
- **Posición de lectura** (`position`): línea y altura, o solo altura, que se guardan al ocultar una pestaña y se restauran al recargarla.

## Seguridad y escritura

- **Frontera de confianza**: Rust decide qué puede hacer cada página por su URL (`vault::access`). Solo las páginas de la bóveda (URL con prefijo `vault::base()`) pueden escribir en ella. Las páginas sueltas comparten el prefijo, pero solo usan la consola y `visor.send`. Las webs externas no son de confianza: lo que publiquen podría llegar a un agente como inyección de instrucciones.
- **Página de la bóveda**: página servida desde la bóveda. Puede tener anotaciones y escritura.
- **Escritura propia** (`own_write`): archivo que visor acaba de escribir. Se reconoce por el hash de su contenido, no por el tiempo, para no tapar los cambios de un agente.

## Términos que debes evitar

- «usuario» en la interfaz y la ayuda: di «persona». (`AGENTS.md` y `docs/decisiones.md` dicen «usuario» para el propietario del proyecto; no mezcles ambos en un mismo texto.)
- «panel» para un lado de la vista dividida: el panel es el árbol lateral; cada mitad es un lado.
- «barra lateral» para el panel.
- «activa» para una pestaña con webview: eso es una pestaña viva. La pestaña activa es la visible de un lado.
- «comentario» para toda la anotación: el comentario es solo el texto que escribe la persona.
- «muestra» o «HTML aislado» para una página suelta.
