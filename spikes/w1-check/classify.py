#!/usr/bin/env python3
"""Agrupa los errores de `cargo check --message-format=json` por categoría y archivo (experimento W1).

Uso: cargo check --message-format=json | python3 classify.py [--stderr FICHERO]

Escribe Markdown en la salida estándar. Sale con código 1 si la entrada no contiene
el mensaje final `build-finished` de cargo (entonces no hay nada que clasificar).

Categorías, en este orden (gana la primera que casa con el mensaje de rustc):
- dependencias de interfaz: gtk, webkit, wry, tao, cairo, rfd, gio, page_view, crate::ui
  (lo que queda de la interfaz tras el recorte; también son costuras, no fallos de Windows)
- libc: menciones a la crate `libc`
- std::os::unix: `std::os::unix` o el módulo `unix` no encontrado, y métodos de sus extensiones
  (`dev`, `ino`, `uid`, `from_mode`, `process_group`…) que rustc no encuentra en Windows
- rutas Unix: rutas o semántica de Unix (symlink, /dev/, permisos POSIX)
- otros: el resto
"""

import json
import re
import sys
from collections import Counter, defaultdict

CATEGORIES = [
    ("dependencias de interfaz", r"\b(gtk|webkit2gtk|webkit|wry|tao|cairo|rfd|gio|page_view)\b|crate::ui|unlinked crate"),
    ("libc", r"\blibc\b"),
    ("std::os::unix", r"std::os::unix|os::unix|`unix`|\bUnix(Listener|Stream)\b|named `(dev|ino|nlink|uid|gid|mode|from_mode|process_group|from_raw_fd|as_raw_fd)`"),
    ("rutas Unix", r"symlink|/dev/|posix|permission bits"),
]
ORDER = [name for name, _ in CATEGORIES] + ["otros"]
SKIP = ("aborting due to", "could not compile")


def categorize(message: str) -> str:
    for name, pattern in CATEGORIES:
        if re.search(pattern, message):
            return name
    return "otros"


def read_events(stream):
    """Devuelve (errores, avisos, terminado_con_exito); cada error es una tupla de datos."""
    errors, warnings, finished, success = [], 0, False, False
    for raw in stream:
        raw = raw.strip()
        if not raw.startswith("{"):
            continue
        try:
            event = json.loads(raw)
        except json.JSONDecodeError:
            continue
        kind = event.get("reason")
        if kind == "build-finished":
            finished, success = True, bool(event.get("success"))
        elif kind == "compiler-message":
            msg = event.get("message", {})
            level = msg.get("level")
            text = msg.get("message", "")
            if level == "warning":
                warnings += 1
            if level != "error" or text.startswith(SKIP):
                continue
            code = (msg.get("code") or {}).get("code") or "-"
            spans = msg.get("spans") or []
            primary = next((s for s in spans if s.get("is_primary")), spans[0] if spans else None)
            location = f"{primary['file_name']}:{primary['line_start']}" if primary else "(sin archivo)"
            file = primary["file_name"] if primary else "(sin archivo)"
            errors.append({"file": file, "location": location, "code": code, "message": text, "category": categorize(text)})
    return errors, warnings, finished, success


def md_cell(text: str) -> str:
    return text.replace("|", "\\|").replace("\n", " ")


def render(errors, warnings, success, stderr_lines) -> str:
    out = ["# Clasificación de `cargo check` (W1)", ""]
    out.append(f"- Errores de rustc: **{len(errors)}**")
    out.append(f"- Avisos de rustc (no contados como errores): {warnings}")
    out.append(f"- Resultado de cargo: {'correcto' if success else 'fallido'}")
    out.append("")

    counts = Counter(e["category"] for e in errors)
    files = defaultdict(Counter)
    for e in errors:
        files[e["file"]][e["category"]] += 1

    out += ["## Resumen por categoría", "", "| Categoría | Errores |", "|---|---:|"]
    for name in ORDER:
        out.append(f"| {name} | {counts.get(name, 0)} |")
    out += ["", "## Por archivo", "", "| Archivo | Errores | Categorías |", "|---|---:|---|"]
    for file in sorted(files, key=lambda f: (-sum(files[f].values()), f)):
        parts = ", ".join(f"{name} {n}" for name, n in files[file].most_common())
        out.append(f"| `{file}` | {sum(files[file].values())} | {parts} |")

    out += ["", "## Lista completa", ""]
    if not errors:
        out.append("Sin errores de rustc.")
    for name in ORDER:
        items = [e for e in errors if e["category"] == name]
        if not items:
            continue
        out += [f"### {name} ({len(items)})", "", "| Ubicación | Código | Mensaje |", "|---|---|---|"]
        for e in items:
            out.append(f"| `{e['location']}` | {e['code']} | {md_cell(e['message'])} |")
        out.append("")

    if not success and not errors:
        out += ["## Fallo sin errores de rustc", "", "cargo falló antes de compilar el crate. Salida de stderr:", "", "```", *stderr_lines[-60:], "```", ""]
    return "\n".join(out) + "\n"


def main() -> int:
    stderr_lines = []
    args = sys.argv[1:]
    if args[:1] == ["--stderr"]:
        if len(args) != 2:
            print("Uso: --stderr FICHERO", file=sys.stderr)
            return 2
        with open(args[1], encoding="utf-8", errors="replace") as f:
            stderr_lines = [line.rstrip("\n") for line in f]
    elif args:
        print(__doc__, file=sys.stderr)
        return 2

    errors, warnings, finished, success = read_events(sys.stdin)
    if not finished:
        print("La salida no contiene build-finished: cargo no terminó. Nada que clasificar.", file=sys.stderr)
        return 1
    sys.stdout.write(render(errors, warnings, success, stderr_lines))
    return 0


if __name__ == "__main__":
    sys.exit(main())
