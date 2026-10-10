# visor

Visor de bóvedas (carpetas) de HTML y Markdown para estudiar con IA, al estilo de Obsidian. También abre un HTML suelto, sin bóveda. Rust + tao + wry sobre WebKitGTK, Wayland nativo (Omarchy/Hyprland); solo compila en Linux. Los agentes lo controlan por CLI; `visor help` es la referencia de esa interfaz y de cómo escribir páginas para visor: si cambias su comportamiento, actualiza `src/agent/help.txt`.

## Mapa

Los términos del dominio (bóveda, lado, pestaña viva, archivo suelto…) están en `CONTEXT.md`.

- `src/session.rs`: el estado que comparten la ventana y los servidores (`Shared`, `Page`, eventos, consola) y lo que el servidor de agentes pide a la ventana (`AgentRequest`). No conoce GTK; nadie fuera de `ui/app.rs` importa de `ui::app`.
- `src/ui/`: la ventana. `app.rs` orquesta webviews, pestañas vivas y descargadas y eventos; `action.rs`, los eventos y acciones que llegan a su bucle; `watch.rs`, la vigilancia de archivos y la decisión pura de qué hacer con un lote de cambios (`vault_reaction`); `page_view.rs`, el webview de una página; `page_calls.rs`, el JavaScript con que Rust llama a las páginas; `panes.rs`, cada lado de la vista dividida con su fila de pestañas, sus botones de anotaciones (solo en páginas de la bóveda) y su barra (`bar.rs`); `sidebar.rs`, el panel en GTK nativo; `icons.rs`, iconos dibujados con cairo (cargarlos del tema cuesta ~50 ms en frío); `theme.rs`, colores de Omarchy.
- `src/vault.rs`: la bóveda abierta (`Vault`, raíz canónica una sola vez) y su frontera de confianza: toda ruta o URL que llega de una página, del panel o de la CLI se valida aquí (`resolve`, `page`, `note`, `entry`, `new_in`, `access`), más `write_atomic`. Dentro, `src/vault/`: `config.rs` (configuración en JSON: bóvedas, espacios de trabajo), `files.rs` (mover y mandar a la papelera), `links.rs` (reescribe enlaces al mover), `markdown.rs` (notas .md en lectura), `annot.rs` (anotaciones; solo Rust escribe `.visor/anotaciones/`).
- `src/agent/`: lo que usan los agentes. `command.rs` define cada orden una vez (`Command`; esquemas MCP en `command/tools.rs`); `cli.rs` y `server.rs` las llevan por socket Unix; `mcp.rs` es el servidor MCP por stdio (`visor mcp`); `help.txt` es `visor help`.
- `src/http.rs`: servidor local que sirve la bóveda, `assets/` y las carpetas de los HTML sueltos (pestañas con ruta absoluta). `src/paths.rs`: socket, configuración y registro de eventos.
- `src/page/`: lo que corre dentro de las páginas. `bridge.js` (consola y `visor.send`), `annot.js` (capa de anotaciones), `note.js/css` (lectura de notas), `editor.js/css` (editor de Ctrl+E, CodeMirror 6, solo se carga al editar).
- `build.rs`: empaqueta `assets/` (KaTeX, highlight.js, Mermaid, CodeMirror) y `editor.js/css` comprimidos con brotli. CodeMirror se regenera en `tools/codemirror/`; las medidas, con `tools/medir/medir.py`.

## Reglas

- **Rendimiento primero.** Mide arranque, memoria (PSS de todo el árbol de procesos), CPU en reposo y peso del binario antes y después de cada cambio, y di el coste de cada función nueva antes de añadirla. Cifras de referencia y método: `docs/pruebas.md`.
- **Frontera de confianza.** `bridge.js` y `annot.js` se inyectan en todas las páginas, pero solo actúan en las que sirve visor (URL con prefijo `vault::base()`). Rust valida cada mensaje IPC contra la URL de la página que lo envía (`vault::access`). Los HTML sueltos comparten ese prefijo y solo pueden usar la consola y `visor.send`: nada que escriba en una bóveda. Las webs externas son *no confiables*: lo que publiquen acabaría en el contexto de un agente (inyección de instrucciones).
- **Textos de interfaz en español** (norma UNE-ISO 24495-1, lenguaje claro); comentarios del código en inglés y solo para el porqué.

## Antes de…

- **probar o medir** cualquier cosa: lee `docs/pruebas.md`. El usuario suele tener visor abierto y tus órdenes irían a su ventana.
- **tocar arquitectura, dependencias o perfil de compilación**: lee `docs/decisiones.md` (índice de los ADR de `docs/adr/`); recoge lo ya medido y decidido con el usuario.
