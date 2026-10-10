# visor

visor es un visor de bóvedas (carpetas) de HTML y Markdown para estudiar con IA, al estilo de Obsidian. También abre un HTML suelto, sin bóveda. Los agentes lo controlan por línea de órdenes o por MCP.

Está escrito en Rust con tao y wry sobre WebKitGTK. Solo compila en Linux, con Wayland nativo.

## Requisitos

- Linux con Wayland.
- WebKitGTK 4.1, versión 2.38 o posterior (`Cargo.toml` activa la feature `v2_38`).
- GTK 3.
- Para vídeo web: `gst-libav` y `gst-plugins-bad`. Se recomiendan `gst-plugin-va` y `gst-plugin-dav1d` ([detalle](docs/adr/0032-requisitos-de-gstreamer.md)).

## Compilar

```
cargo build --release
```

El paquete de CodeMirror ya está en `assets/`. Solo se regenera con `npm ci && npm run build` en `tools/codemirror/`.

## Uso

```
visor                              # abre la app
visor open <carpeta|archivo|URL>   # abre una bóveda, una página o una web
visor help                         # referencia completa
visor mcp                          # servidor MCP por stdio
```

`visor open` admite `--tab` (pestaña nueva) y `--beside` (en el otro lado).

Para usar visor desde un cliente MCP, regístralo así (ejemplo de Claude Code, tomado de `visor help`):

```
claude mcp add visor -- visor mcp
```

### Atajos principales

| Atajo | Acción |
|---|---|
| Ctrl+O | abrir carpeta |
| Ctrl+B | mostrar u ocultar el panel |
| Ctrl+L | ir a la barra de direcciones |
| Ctrl+E | editar la nota o volver a la vista |
| Ctrl+T, Ctrl+W | pestaña nueva, cerrar pestaña |
| Ctrl+\| (o Ctrl+\\) | dividir la ventana o volver a un lado |
| Ctrl+Shift+A, Ctrl+Shift+L | anotar, lista de anotaciones |

La lista completa está en `visor help`.

## Más documentación

- [CONTEXT.md](CONTEXT.md): glosario de términos.
- [docs/decisiones.md](docs/decisiones.md) y [docs/adr/](docs/adr/): decisiones de diseño y sus cifras.
- [docs/pruebas.md](docs/pruebas.md): cómo probar y medir.
- [AGENTS.md](AGENTS.md): guía para agentes que programan en este repositorio.

## Licencia

MIT. Consulta [LICENSE](LICENSE).
