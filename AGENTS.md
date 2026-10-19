# visor

Visor de bóvedas (carpetas) de HTML y Markdown para estudiar con IA, al estilo de Obsidian. Rust + tao + wry sobre WebKitGTK, Wayland nativo; solo compila en Linux. Los agentes lo controlan por CLI o MCP. `src/agent/help.txt` es `visor help` y la herramienta MCP `guide`: si cambias un comportamiento visible, actualízalo. Términos del dominio: `CONTEXT.md`.

## Mapa

Solo lo que el código no dice por sí mismo.

- `src/ui/visor.rs`: el proceso. Abre, cierra y guarda las ventanas, y reparte cada evento (`Routed`) a su `App` (`app.rs`, una por ventana). La ventana 0 es el proceso. Lo común a todas las ventanas está en `ui/common.rs`, un `RefCell`: nunca mantengas un préstamo durante una llamada a `App`. `ui/windows.rs`: modelo de ventanas sin GTK (ADR 0039).
- `src/session.rs`: estado compartido entre ventanas y servidores, por ventana. Es un `Mutex` no reentrante: no llames a un método que lo tome mientras lo tienes. Ni él ni `agent/` dependen de GTK.
- `src/ui/tabs.rs`: modelo de pestañas sin GTK ni wry. Cada operación devuelve efectos que `app.rs` aplica; los lados a redibujar se marcan y se redibujan una vez por evento. Añade lógica de pestañas aquí, con prueba, no en `app.rs`.
- `src/ui/watch.rs`: `vault_reaction` decide, sin tocar disco real (trait `Disk`), qué hacer con un lote de cambios.
- `src/ui/icons.rs`: iconos con cairo, ninguno del tema (ADR 0004).
- `src/vault.rs`: frontera de confianza. Toda ruta o URL que llega de una página, del panel o de la CLI se valida aquí (`resolve`, `page`, `page_at`, `note`, `entry`, `new_in`, `access`). Escribe con `write_atomic` (sigue enlaces y conserva permisos). Todo recorrido de carpetas pasa por `Walk`/`files` (ocultos, bucles y enlaces hacia fuera, en un solo sitio). Un archivo ilegible (`config.json`, anotaciones) nunca se trata como vacío: el primero se copia a `.bak`, las segundas dan error. `vault/annot.rs`: solo Rust escribe `.visor/anotaciones/`.
- `src/vault/drawing.rs` y `src/page/drawing*.js`: dibujos de Excalidraw (`assets/excalidraw/`, solo en sus páginas). El formato de `.excalidraw.md` está en `page/drawing-format.js`, sin DOM: pruebas con `node --test tools/excalidraw/test/drawing-format.test.mjs`. `drawing::accepts` impide que un texto sin forma de dibujo sustituya al archivo.
- `src/agent/command.rs`: cada orden se define una vez (`Command`); CLI, socket y MCP salen de ahí (esquemas MCP en `command/tools.rs`). Una orden nueva se añade solo allí y en `help.txt`.
- `src/platform/`: lo que cada sistema hace a su manera (identidad de archivo, papelera, procesos). `agent/channel.rs`: el canal de la CLI (socket Unix; tubería con nombre en Windows). Lo propio de un sistema va ahí, no con `#[cfg]` sueltos. Plan del port: `docs/plan-multiplataforma.md`.
- `src/http.rs`: sirve la bóveda, `assets/` y las carpetas de los HTML sueltos. `src/paths.rs`: socket, configuración, registro de eventos. `agent/socket.rs`: una sola instancia posee el socket (bloqueo de archivo); un segundo `visor` le pide otra ventana.
- `src/page/`: JavaScript y CSS de las páginas. `editor.js/css` solo se carga al editar. Dentro del `contenteditable` del editor no puede haber elementos reemplazados (`<img>`, `<input>`, `<math>`) ni con rol hoja (`role="img"`, `<button>`): con un cliente AT-SPI escuchando, WebKit aborta (ADR 0038).
- `build.rs`: comprime con brotli `assets/` y `editor.js/css`. CodeMirror se regenera en `tools/codemirror/` (ADR 0028). Medidas: `tools/medir/medir.py`.

## Reglas

- **Rendimiento primero.** Antes y después de cada cambio mide arranque, memoria (PSS de todo el árbol de procesos), CPU en reposo y peso del binario. Di el coste de cada función nueva antes de añadirla. Método y cifras: `docs/pruebas.md`.
- **Frontera de confianza.** Lo que puede hacer una página lo decide Rust (`vault::access`) con la URL del documento que se cargó en su vista, registrada al llegar a `Committed` (`ui/page_view.rs`). Nunca con `webview.uri()` ni con la URL que trae el mensaje: `history.pushState` y las cargas pendientes la cambian. El canal IPC llega a todos los marcos, así que Rust solo acepta mensajes con la clave de la vista, que solo conocen los scripts de visor del marco principal (`window.__visorPost` en `bridge.js`). Cada bóveda se sirve en su propia dirección del bucle local (127.0.0.1, .3, .4…, con su clave), para que una página no lea otra bóveda. Los HTML sueltos se sirven en 127.0.0.2: otro origen, para que no lean la bóveda ni tomen la dirección de una de sus páginas. `vault::access` solo confía en las páginas de la bóveda de la ventana de la vista. Un suelto solo lee su carpeta y subcarpetas, nada oculto, y no escribe en la bóveda. Las webs externas son *no confiables*: lo que publiquen llegaría al contexto de un agente (inyección de instrucciones), así que no tienen `visor.send` ni consola registrada. `visor eval` y `visor screenshot` leen la misma URL (`vault::agent_may_read`); el JS de `eval` comprueba además que el documento vivo sigue siendo de visor, por si la vista ya cargó otra cosa.
- **Textos de interfaz en español** (UNE-ISO 24495-1, lenguaje claro). Comentarios del código en inglés y solo para el porqué.
- **Idioma del repositorio.** Commits y títulos de PR en inglés (formato en `CONTRIBUTING.md`); issues en español. El resto de la documentación sigue en español.

## Antes de…

- **probar o medir**: lee `docs/pruebas.md`. El usuario suele tener visor abierto; tus órdenes irían a su ventana si no aíslas la instancia.
- **tocar arquitectura, dependencias o perfil de compilación**: lee el ADR afectado en `docs/adr/` (el nombre de cada archivo dice de qué trata; 0034 reúne las decisiones de una línea, y los valores por defecto de WebKit que no se tocan están comentados junto a `WebContext::new` en `ui/visor.rs`). Cada ADR recoge lo medido o acordado con el usuario; cambia uno solo con datos nuevos y consultándolo. Los que citan cifras (0003, 0004, 0011, 0013, 0019, 0026, 0028, 0036, 0037) se re-miden con `docs/pruebas.md`.
