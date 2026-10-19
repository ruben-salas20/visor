#!/usr/bin/env python3
"""Mide los tres marcos del prototipo U1 (simple, react, svelte) en un sway sin pantalla.

Uso: medir.py [--rondas N] [SALIDA.jsonl]

Por ronda y marco: tiempo hasta que el marco avisa de que está listo y hasta que pinta
el árbol (medido dentro del proceso, desde el inicio de main), memoria PSS de todo el
árbol de procesos 10 s después de pintar, y CPU en reposo durante 30 s. Las páginas
quedan vacías (about:blank): se mide el marco, no las notas. El orden de los marcos
rota en cada ronda, y cada marco tiene antes un arranque de calentamiento que no cuenta.
Método de PSS y CPU: el mismo que tools/medir/medir.py.
"""

import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent.parent
BINARIO = HERE / "target" / "release" / "u1-marco-html"
MARCOS = ["simple", "react", "svelte"]
TICK = os.sysconf("SC_CLK_TCK")
UNIDAD = f"u1-medir-sway-{os.getpid()}"


def tree(pid):
    children = {}
    for p in Path("/proc").iterdir():
        if p.name.isdigit():
            try:
                ppid = int((p / "stat").read_text().rsplit(")", 1)[1].split()[1])
            except (OSError, IndexError):
                continue
            children.setdefault(ppid, []).append(int(p.name))
    out, todo = [], [pid]
    while todo:
        q = todo.pop()
        out.append(q)
        todo.extend(children.get(q, []))
    return out


def pss_mb(pid):
    total = 0
    for q in tree(pid):
        try:
            for line in Path(f"/proc/{q}/smaps_rollup").read_text().splitlines():
                if line.startswith("Pss:"):
                    total += int(line.split()[1])
        except OSError:
            pass
    return round(total / 1024, 1)


def cpu_ticks(pid):
    total = 0
    for q in tree(pid):
        try:
            f = Path(f"/proc/{q}/stat").read_text().rsplit(")", 1)[1].split()
            total += int(f[11]) + int(f[12])
        except (OSError, IndexError):
            pass
    return total


def start_sway(tmp):
    display = tmp / "display"
    (tmp / "sway.conf").write_text(
        "output HEADLESS-1 resolution 1920x1080\n"
        "xwayland disable\n"
        f"exec sh -c 'echo $WAYLAND_DISPLAY > {display}'\n")
    subprocess.run(["systemd-run", "--user", "--quiet", "--collect", f"--unit={UNIDAD}",
                    "-E", "WLR_BACKENDS=headless", "-E", "WLR_LIBINPUT_NO_DEVICES=1",
                    "-E", f"SWAYSOCK={tmp / 'sway.sock'}", "-E", f"XDG_RUNTIME_DIR={os.environ['XDG_RUNTIME_DIR']}",
                    "sway", "-c", str(tmp / "sway.conf")], check=True)
    deadline = time.time() + 10
    while not (display.exists() and display.read_text().strip()):
        if time.time() > deadline:
            sys.exit(f"sway no arrancó: falta {display}")
        time.sleep(0.1)
    env = dict(os.environ, WAYLAND_DISPLAY=display.read_text().strip(), SWAYSOCK=str(tmp / "sway.sock"))
    env.pop("DISPLAY", None)
    return env


def una(env, tmp, marco, cuenta):
    tiempos = tmp / f"tiempos-{marco}.txt"
    tiempos.unlink(missing_ok=True)
    proc = subprocess.Popen([str(BINARIO), "--marco", marco], env=dict(env, U1_TIEMPOS=str(tiempos)),
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        deadline = time.time() + 20
        datos = {}
        while "pintado" not in datos:
            if time.time() > deadline:
                return {"marco": marco, "error": "no pintó en 20 s"}
            time.sleep(0.05)
            if tiempos.exists():
                datos = dict(l.split() for l in tiempos.read_text().splitlines() if l.strip())
        if not cuenta:
            return None
        time.sleep(10)
        r = {"marco": marco, "listo_ms": float(datos["listo"]), "pintado_ms": float(datos["pintado"]),
             "memoria_mb": pss_mb(proc.pid), "procesos": len(tree(proc.pid))}
        antes = cpu_ticks(proc.pid)
        time.sleep(30)
        r["cpu_reposo_pct"] = round((cpu_ticks(proc.pid) - antes) / TICK / 30 * 100, 3)
        return r
    finally:
        proc.terminate()
        try:
            proc.wait(5)
        except subprocess.TimeoutExpired:
            proc.kill()


def main():
    args = sys.argv[1:]
    rondas = 3
    if "--rondas" in args:
        i = args.index("--rondas")
        rondas = int(args[i + 1])
        del args[i:i + 2]
    salida = Path(args[0]) if args else Path(tempfile.gettempdir()) / f"u1-medir-{time.strftime('%Y%m%d-%H%M%S')}.jsonl"
    tmp = Path(tempfile.mkdtemp(prefix="u1-medir-"))
    env = start_sway(tmp)
    try:
        for marco in MARCOS:
            una(env, tmp, marco, cuenta=False)
        for ronda in range(rondas):
            orden = MARCOS[ronda % 3:] + MARCOS[:ronda % 3]
            for marco in orden:
                r = una(env, tmp, marco, cuenta=True)
                r["ronda"] = ronda + 1
                print(json.dumps(r, ensure_ascii=False), flush=True)
                with salida.open("a") as f:
                    f.write(json.dumps(r, ensure_ascii=False) + "\n")
    finally:
        subprocess.run(["systemctl", "--user", "stop", UNIDAD], capture_output=True)
        shutil.rmtree(tmp, ignore_errors=True)
    print(f"Resultados: {salida}")


if __name__ == "__main__":
    main()
