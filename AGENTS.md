# visor

Visor de bóvedas (carpetas) de HTML y Markdown para estudiar con IA, al estilo de Obsidian. Rust + tao + wry sobre WebKitGTK, Wayland nativo; solo compila en Linux. Los agentes lo controlan por CLI o MCP. `src/agent/help.txt` es `visor help` y la herramienta MCP `guide`: si cambias un comportamiento visible, actualízalo. Términos del dominio: `CONTEXT.md`.

## Mapa

Solo lo que el código no dice por sí mismo.

- `src/session.rs`: estado compartido entre ventana y servidores. Ni él ni `agent/` dependen de GTK.
- `src/ui/tabs.rs`: modelo de pestañas sin GTK ni wry. Cada operación devuelve efectos que `app.rs` aplica; los lados a redibujar se marcan y se redibujan una vez por evento. Añade lógica de pestañas aquí, con prueba, no en `app.rs`.
- `src/ui/watch.rs`: `vault_reaction` decide, sin tocar disco real (trait `Disk`), qué hacer con un lote de cambios.
- `src/ui/icons.rs`: iconos con cairo, ninguno del tema (ADR 0004).
- `src/vault.rs`: frontera de confianza. Toda ruta o URL que llega de una página, del panel o de la CLI se valida aquí (`resolve`, `page`, `note`, `entry`, `new_in`, `access`). Escribe con `write_atomic`. `vault/annot.rs`: solo Rust escribe `.visor/anotaciones/`.
- `src/agent/command.rs`: cada orden se define una vez (`Command`); CLI, socket y MCP salen de ahí (esquemas MCP en `command/tools.rs`). Una orden nueva se añade solo allí y en `help.txt`.
- `src/http.rs`: sirve la bóveda, `assets/` y las carpetas de los HTML sueltos. `src/paths.rs`: socket, configuración, registro de eventos.
- `src/page/`: JavaScript y CSS de las páginas. `editor.js/css` solo se carga al editar.
- `build.rs`: comprime con brotli `assets/` y `editor.js/css`. CodeMirror se regenera en `tools/codemirror/` (ADR 0028). Medidas: `tools/medir/medir.py`.

## Reglas

- **Rendimiento primero.** Antes y después de cada cambio mide arranque, memoria (PSS de todo el árbol de procesos), CPU en reposo y peso del binario. Di el coste de cada función nueva antes de añadirla. Método y cifras: `docs/pruebas.md`.
- **Frontera de confianza.** `bridge.js` y `annot.js` se inyectan en todas las páginas, pero solo actúan en las que sirve visor (URL con prefijo `vault::base()`). Rust valida cada mensaje IPC contra la URL de quien lo envía (`vault::access`). Los HTML sueltos comparten ese prefijo y solo usan la consola y `visor.send`: no escriben en la bóveda. Las webs externas son *no confiables*: lo que publiquen llegaría al contexto de un agente (inyección de instrucciones), así que no tienen `visor.send` ni consola registrada.
- **Textos de interfaz en español** (UNE-ISO 24495-1, lenguaje claro). Comentarios del código en inglés y solo para el porqué.

## Antes de…

- **probar o medir**: lee `docs/pruebas.md`. El usuario suele tener visor abierto; tus órdenes irían a su ventana si no aíslas la instancia.
- **tocar arquitectura, dependencias o perfil de compilación**: lee `docs/decisiones.md` y el ADR afectado. Cada ADR recoge lo medido o acordado con el usuario; cambia uno solo con datos nuevos y consultándolo.
