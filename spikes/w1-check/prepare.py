#!/usr/bin/env python3
"""Copia visor sin la interfaz gráfica para pasar `cargo check` a Windows (experimento W1).

Uso: prepare.py DESTINO [--control] [--stub-seams] [--drop-unix-imports]

Por defecto quita de la copia la interfaz (GTK, WebKitGTK, wry, tao, rfd, cairo),
deja solo los módulos portables de `ui/` y quita `visor` de `main.rs`.

Pasadas (se acumulan, cada una sobre la anterior):
- --stub-seams: sustituye en la copia las tres costuras de interfaz por rellenos con
  `unimplemented!()` y la misma firma: `ui::page_view::is_web`, la papelera de GTK en
  `vault/files.rs` y `tao::event_loop::EventLoopProxy` en `ui/action.rs`. Así los errores
  de resolución desaparecen y rustc llega a la comprobación de tipos.
- --drop-unix-imports: borra los `use std::os::unix…` de nivel superior. Los métodos de
  extensión (`dev`, `ino`, `from_mode`, `process_group`) fallan de nuevo; lo que cambia
  es qué errores aparecen detrás de los imports que fallaban.

Con --control, la copia conserva todas las dependencias y añade `ui/page_view.rs`
(el único módulo que da `is_web`). Sirve de control en Linux: si compila, el
recorte no introduce errores por sí mismo. No se combina con las otras pasadas.
"""

import re
import shutil
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
SRC = HERE.parent.parent  # raíz del worktree

PORTABLE_UI = ["action", "address", "page_calls", "tabs", "watch", "windows"]
CONTROL_UI = PORTABLE_UI + ["page_view"]
STUB_UI = PORTABLE_UI + ["page_view"]
UI_DEPS = {"cairo-rs", "gtk", "rfd", "tao", "webkit2gtk", "wry"}

MAIN_OLD = "visor::launch()"
MAIN_NEW = '{\n            eprintln!("visor sin interfaz (copia de la prueba W1): no hay ventana que abrir.");\n            1\n        }'

PAGE_VIEW_STUB = """//! Relleno de `ui::page_view` para la prueba W1 (copia generada): solo lo que importan los módulos portables.
pub fn is_web(_url: &str) -> bool {
    unimplemented!()
}
"""

# (archivo, texto exacto que se sustituye, texto nuevo). Cada texto debe aparecer una sola vez.
SEAM_EDITS = [
    ("src/ui/action.rs", "use tao::event_loop::EventLoopProxy;\n", """/// Relleno de `tao::event_loop::EventLoopProxy` para la prueba W1 (copia generada).
pub struct EventLoopProxy<T>(std::marker::PhantomData<fn() -> T>);
impl<T> Clone for EventLoopProxy<T> {
    fn clone(&self) -> Self {
        Self(std::marker::PhantomData)
    }
}
impl<T> EventLoopProxy<T> {
    pub fn send_event(&self, _event: T) -> Result<(), ()> {
        unimplemented!()
    }
}
"""),
    ("src/vault/files.rs", "    use gtk::gio::{Cancellable, File, prelude::FileExt};\n", ""),
    ("src/vault/files.rs", "    File::for_path(&path).trash(Cancellable::NONE).map_err(", "    seam_trash(&path).map_err("),
    ("src/vault/files.rs", "        let _ = File::for_path(&notes).trash(Cancellable::NONE);", "        let _ = seam_trash(&notes);"),
]
# Función de relleno que se añade al final de vault/files.rs (sustituye a la papelera de GTK).
TRASH_STUB = """
/// Relleno de la papelera de GTK para la prueba W1 (copia generada).
fn seam_trash(_path: &Path) -> Result<(), String> {
    unimplemented!()
}
"""

UNIX_IMPORT = re.compile(r"(?m)^use std::os::unix[^\n]*\n")


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


def apply_seams(dest: Path) -> None:
    for rel, old, new in SEAM_EDITS:
        path = dest / rel
        text = path.read_text(encoding="utf-8")
        if text.count(old) != 1:
            raise SystemExit(f"{rel}: esperaba una vez el texto a sustituir; revisa prepare.py\n{old!r}")
        path.write_text(text.replace(old, new), encoding="utf-8")
    files_rs = dest / "src" / "vault" / "files.rs"
    files_rs.write_text(files_rs.read_text(encoding="utf-8") + TRASH_STUB, encoding="utf-8")


def drop_unix_imports(dest: Path) -> int:
    removed = 0
    for path in (dest / "src").rglob("*.rs"):
        text = path.read_text(encoding="utf-8")
        new, n = UNIX_IMPORT.subn("", text)
        if n:
            path.write_text(new, encoding="utf-8")
            removed += n
    return removed


def main() -> int:
    args = sys.argv[1:]
    flags = {"--control", "--stub-seams", "--drop-unix-imports"}
    control = "--control" in args
    stub = "--stub-seams" in args
    drop = "--drop-unix-imports" in args
    args = [a for a in args if a not in flags]
    if len(args) != 1 or any(a.startswith("--") for a in args):
        print(__doc__, file=sys.stderr)
        return 2
    if control and (stub or drop):
        print("--control no se combina con --stub-seams ni --drop-unix-imports", file=sys.stderr)
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
    modules = CONTROL_UI if control else (STUB_UI if stub else PORTABLE_UI)
    for name in modules:
        if stub and name == "page_view":
            (ui_dir / "page_view.rs").write_text(PAGE_VIEW_STUB, encoding="utf-8")
        else:
            shutil.copy2(SRC / "src" / "ui" / f"{name}.rs", ui_dir / f"{name}.rs")
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

    if stub:
        apply_seams(dest)
    if drop:
        print(f"Imports std::os::unix de nivel superior quitados: {drop_unix_imports(dest)}")

    if control:
        modo = "control Linux"
    else:
        modo = "Windows, sin interfaz"
        if stub:
            modo += ", costuras rellenas"
        if drop:
            modo += ", sin imports unix"
    print(f"Copia lista en {dest} (modo: {modo}; ui: {', '.join(sorted(modules))})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
