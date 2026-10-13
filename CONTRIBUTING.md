# Contributing to visor

Thanks for helping. You can write to us in **English or Spanish**: issues, pull requests and reviews are welcome in either language. También recibimos aportes en español.

## Languages in this repository

- **Commit messages and pull request titles**: English. A pull request title often ends up in the history as a commit message.
- **Issues and discussions**: English or Spanish. The maintainer writes issues in Spanish.
- **Code comments**: English, and only to explain why.
- **Internal documentation** (`AGENTS.md`, `CONTEXT.md`, `docs/adr/`, `docs/pruebas.md`): Spanish for now. If you need part of it in English, ask in an issue.
- **Interface text**: Spanish for now. English is planned ([#24](https://github.com/ruben-salas20/visor/issues/24)).

## Before you start

- For a new feature or a large change, open an issue first, so we can agree on the approach before you write code.
- Read [`AGENTS.md`](AGENTS.md). It is the map of the code and lists the rules every change follows. It is written for AI agents, but it is the shortest guide for people too.
- Design decisions live in [`docs/adr/`](docs/adr/), each with the measurements behind it. To change one, bring new data.

## Build and test

visor builds only on Linux. You need Rust 1.89 or later, GTK 3 and WebKitGTK 4.1.

```
cargo build --release
cargo test
node --test tools/excalidraw/test/drawing-format.test.mjs
```

If you already use visor, run your build as a separate instance. Otherwise your commands reach your open window and write to your configuration. [`docs/pruebas.md`](docs/pruebas.md) shows how.

## Performance

visor puts performance first. For every change, measure startup time, memory, idle CPU and binary size, before and after, with `tools/medir/medir.py` (method in [`docs/pruebas.md`](docs/pruebas.md)). Put the figures in the pull request. If you add a feature, say what it costs.

## Security

Every path or URL that comes from a page, the panel or the command line is checked in `src/vault.rs`. Do not add a way around it. Pages from the web are untrusted: what they publish could reach an AI agent. If you find a security problem, do not open a public issue: report it privately through [GitHub security advisories](https://github.com/ruben-salas20/visor/security/advisories/new).

## Commit messages

One line in English, starting with the area it touches, and the issue number if there is one:

```
Bridge: forward the kit:review event with visor.send (#33)
```

## License

By contributing, you agree that your work is published under the [MIT license](LICENSE).
