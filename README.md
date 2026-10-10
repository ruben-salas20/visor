# visor

[Español](README.es.md)

A desktop viewer for vaults (folders) of HTML and Markdown pages, built for studying with AI, in the spirit of Obsidian. It also opens a single loose HTML file without a vault. Agents drive it through a CLI or MCP; the person reads, highlights, comments, and gets replies from the agent.

Rust + tao + wry on WebKitGTK. Linux only, native Wayland.

## Requirements

- Linux with Wayland, GTK 3, WebKitGTK 4.1 (2.38 or later).
- Web video: `gst-libav` and `gst-plugins-bad`; `gst-plugin-va` and `gst-plugin-dav1d` recommended ([why](docs/adr/0032-requisitos-de-gstreamer.md)).
- Rust 1.89 or later to build.

## Build

```
cargo build --release
```

The CodeMirror bundle is committed in `assets/`; regenerate it only with `npm ci && npm run build` in `tools/codemirror/`.

## Use

```
visor                              # open the app
visor open <folder|file|URL> [--tab | --beside]
visor help                         # full CLI reference and how to write pages
visor mcp                          # MCP server over stdio
```

Register it with an MCP client, for example:

```
claude mcp add visor -- visor mcp
```

The MCP tool `guide` returns the same text as `visor help`, shortcuts included (Ctrl+O open folder, Ctrl+E edit note, Ctrl+Shift+A annotate, Ctrl+| split).

## Repository layout

- `src/ui/`: window, tabs, sidebar (GTK).
- `src/vault.rs`, `src/vault/`: vault access, annotations, Markdown, config.
- `src/agent/`: CLI, socket server, MCP; `help.txt` is `visor help`.
- `src/page/`: JavaScript and CSS that run inside pages.
- `assets/`, `build.rs`: bundled libraries (KaTeX, highlight.js, Mermaid, CodeMirror), brotli-compressed at build time.
- `tools/`: CodeMirror bundle generator, benchmark script.

## Documentation

All of it is in Spanish.

- [README.es.md](README.es.md): guide for people.
- [CONTEXT.md](CONTEXT.md): glossary.
- [AGENTS.md](AGENTS.md): guide for agents working on this repository.
- [docs/decisiones.md](docs/decisiones.md): design decisions (ADR), with the measurements behind them.
- [docs/pruebas.md](docs/pruebas.md): how to test and measure.

## License

MIT. See [LICENSE](LICENSE).
