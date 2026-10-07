# visor

Visor de bóvedas (carpetas) de HTML y Markdown para estudiar con IA, al estilo de Obsidian. También abre un HTML suelto, sin bóveda. Rust + tao + wry sobre WebKitGTK, Wayland nativo (Omarchy/Hyprland); solo compila en Linux. Los agentes lo controlan por CLI; `visor help` es la referencia de esa interfaz y de cómo escribir páginas para visor: si cambias su comportamiento, actualiza `src/help.txt`.

## Mapa

- `app.rs`: ventana, webviews de las páginas, pestañas vivas y descargadas, eventos, vigilancia de archivos, tema.
- `panes.rs`: cada lado de la vista dividida, con su fila de pestañas, sus botones de anotaciones (solo en páginas de la bóveda) y su barra.
- `http.rs`: servidor local que sirve la bóveda y las carpetas de los HTML sueltos (pestañas con ruta absoluta); `vault.rs`: árbol, rutas, configuración.
- `files.rs`: mover y mandar a la papelera desde el panel; `links.rs`: reescribe los enlaces de las notas al mover.
- `server.rs` + `cli.rs`: órdenes de agentes por socket Unix.
- `markdown.rs` + `note.js/css`: notas .md en lectura. `editor.js/css`: editor de Ctrl+E (CodeMirror 6 con vista previa), que solo se carga al editar. `annot.rs` + `annot.js`: anotaciones.
- `bridge.js`: consola y `visor.send` en las páginas. `sidebar.rs`: panel lateral en GTK nativo. `bar.rs`: barra de direcciones GTK.
- `build.rs`: empaqueta `assets/` (KaTeX, highlight.js, Mermaid, CodeMirror) y `editor.js/css` comprimidos con brotli. CodeMirror se regenera en `tools/codemirror/` (ver `docs/decisiones.md`).

## Reglas

- **Rendimiento primero.** Mide arranque, memoria (PSS de todo el árbol de procesos) y peso del binario antes y después de cada cambio, y di el coste de cada función nueva antes de añadirla. Cifras de referencia y método: `docs/pruebas.md`.
- **Frontera de confianza.** Solo las páginas de la bóveda (URL con prefijo `vault::base()`) reciben `bridge.js`, `annot.js` y voz en el canal IPC; Rust valida cada mensaje contra la URL de la página que lo envía. Los HTML sueltos comparten ese prefijo y solo pueden usar la consola y `visor.send`: nada que escriba en una bóveda. Las webs externas son *no confiables*: lo que publiquen acabaría en el contexto de un agente (inyección de instrucciones).
- **Textos de interfaz en español** (norma UNE-ISO 24495-1, lenguaje claro); comentarios del código en inglés y solo para el porqué.
- **Pruebas manuales** que no puedas hacer tú (ratón, diálogos, webs externas): añade el caso a `boveda-prueba/` y a su lista `00 Empieza aquí.md`.

## Antes de…

- **probar o medir** cualquier cosa: lee `docs/pruebas.md`. El usuario suele tener visor abierto y tus órdenes irían a su ventana.
- **tocar arquitectura, dependencias o perfil de compilación**: lee `docs/decisiones.md`; recoge lo ya medido y decidido con el usuario.
