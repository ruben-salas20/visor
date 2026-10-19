#!/usr/bin/env python3
"""Copia visor sin la interfaz gráfica para pasar `cargo check` a Windows (experimento W1).

Uso: prepare.py DESTINO [--control]

Quita de la copia la interfaz (GTK, WebKitGTK, wry, tao, rfd, cairo), deja solo los
módulos portables de `ui/` y quita `visor` de `main.rs`.

Las tres pasadas de antes (sin relleno, costuras rellenas y sin imports de Unix) se
quitaron cuando la fase 1 puso las costuras en el código (`src/platform/`,
`agent/channel.rs`); sus resultados están en `informe-local.md` y en el plan.

Con --control, la copia conserva todas las dependencias y añade `ui/page_view.rs`.
Sirve de control en Linux: si compila, el recorte no introduce errores por sí mismo.
"""

import re
import shutil
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
SRC = HERE.parent.parent  # raíz del worktree

PORTABLE_UI = ["action", "address", "page_calls", "protocol", "tabs", "watch", "windows"]
# Modules with a folder of submodules (`ui/protocol/`).
PORTABLE_DIRS = ["protocol"]
CONTROL_UI = PORTABLE_UI + ["page_view"]
UI_DEPS = {"cairo-rs", "gtk", "rfd", "tao", "webkit2gtk", "wry"}

MAIN_OLD = "visor::launch()"
MAIN_NEW = '{\n            eprintln!("visor sin interfaz (copia de la prueba W1): no hay ventana que abrir.");\n            1\n        }'

def strip_deps(text: str) -> str:
    """Quita de [dependencies] las líneas de las dependencias de interfaz (y su comentario previo)."""
    out, section = [], None
    for line in text.splitlines(keepends=True):
        header = re.match(r"\s*\[(.+?)\]\s*$", line)
        if header:
            section = header.group(1)
        if section == "dependencies":
            key = re.match(r"\s*([A-Za-z0-9_-]+)\s*=", line)
            if key and key.group(1) in UI_DEPS:
                if out and out[-1].lstrip().startswith("#"):
                    out.pop()
                continue
        out.append(line)
    return "".join(out)


def main() -> int:
    args = sys.argv[1:]
    flags = {"--control"}
    control = "--control" in args
    args = [a for a in args if a not in flags]
    if len(args) != 1 or any(a.startswith("--") for a in args):
        print(__doc__, file=sys.stderr)
        return 2
    dest = Path(args[0]).resolve()
    if dest == SRC or SRC in dest.parents or dest in SRC.parents or dest == Path.home():
        print(f"Destino peligroso: {dest}", file=sys.stderr)
        return 2
    if dest.exists():
        shutil.rmtree(dest)
    dest.mkdir(parents=True)

    for name in ["Cargo.toml", "Cargo.lock", "build.rs"]:
        shutil.copy2(SRC / name, dest / name)
    if not control:
        cargo = dest / "Cargo.toml"
        cargo.write_text(strip_deps(cargo.read_text(encoding="utf-8")), encoding="utf-8")

    shutil.copytree(SRC / "assets", dest / "assets")
    (dest / "docs/img/logo").mkdir(parents=True)
    shutil.copy2(SRC / "docs/img/logo/simbolo.svg", dest / "docs/img/logo/simbolo.svg")  # include_str! in desktop.rs
    shutil.copytree(SRC / "src", dest / "src")

    # ui/: solo los módulos portables. Las rutas crate::ui::… se conservan para que compilen.
    ui_dir = dest / "src" / "ui"
    shutil.rmtree(ui_dir)
    ui_dir.mkdir()
    modules = CONTROL_UI if control else PORTABLE_UI
    for name in modules:
        shutil.copy2(SRC / "src" / "ui" / f"{name}.rs", ui_dir / f"{name}.rs")
        if name in PORTABLE_DIRS:
            shutil.copytree(SRC / "src" / "ui" / name, ui_dir / name)
    ui_mod = ["//! Subconjunto portátil de la interfaz, para la prueba W1 (copia generada)."]
    ui_mod += [f"pub mod {name};" for name in sorted(modules)]
    (dest / "src" / "ui.rs").write_text("\n".join(ui_mod) + "\n", encoding="utf-8")

    main_rs = dest / "src" / "main.rs"
    text = main_rs.read_text(encoding="utf-8")
    for old in ["use ui::visor;\n", MAIN_OLD]:
        if old not in text:
            print(f"main.rs ha cambiado: no encuentro {old!r}; revisa prepare.py", file=sys.stderr)
            return 1
    text = text.replace("use ui::visor;\n", "").replace(MAIN_OLD, MAIN_NEW)
    main_rs.write_text(text, encoding="utf-8")

    modo = "control Linux" if control else "Windows, sin interfaz"
    print(f"Copia lista en {dest} (modo: {modo}; ui: {', '.join(sorted(modules))})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
