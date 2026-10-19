#!/usr/bin/env python3
"""Compara la memoria y la CPU en reposo de visor (interfaz GTK) con el prototipo U1 (marco HTML).

Uso: medir_visor.py VISOR [--rondas N] [SALIDA.jsonl]

Mismas condiciones para todos: sway sin pantalla, la misma página (un h1 y un párrafo)
abierta en los dos lados. Apertura: desde que se lanza el proceso hasta que las dos
páginas han cargado (en visor incluye las dos llamadas a la CLI, ~25 ms cada una; en el
prototipo, hasta que el script de cada página avisa). PSS de todo el árbol de procesos 10 s después de cargar y
CPU en reposo durante 30 s. visor corre aislado (VISOR_SOCKET y XDG_* propios, sin
consulta de actualizaciones). El orden rota en cada ronda; cada variante tiene antes
un arranque de calentamiento que no cuenta.
"""

import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from medir import BINARIO, TICK, cpu_ticks, pss_mb, start_sway, tree  # noqa: E402

PAGINA_PROTOTIPO = "carpeta-01/nota-001.html"
VARIANTES = ["visor", "simple", "react"]


def visor_env(env, tmp):
    e = dict(env, VISOR_SOCKET=str(Path(os.environ["XDG_RUNTIME_DIR"]) / f"u1-visor-{os.getpid()}.sock"),
             VISOR_NO_UPDATE="1")
    for k in ["CONFIG", "STATE", "DATA", "CACHE"]:
        d = tmp / "visor" / k.lower()
        d.mkdir(parents=True, exist_ok=True)
        e[f"XDG_{k}_HOME"] = str(d)
    return e


def medir_proceso(proc, variante, apertura_ms):
    time.sleep(10)
    r = {"variante": variante, "apertura_ms": round(apertura_ms, 1), "memoria_mb": pss_mb(proc.pid), "procesos": len(tree(proc.pid))}
    antes = cpu_ticks(proc.pid)
    time.sleep(30)
    r["cpu_reposo_pct"] = round((cpu_ticks(proc.pid) - antes) / TICK / 30 * 100, 3)
    return r


def parar(proc):
    proc.terminate()
    try:
        proc.wait(5)
    except subprocess.TimeoutExpired:
        proc.kill()


def una_visor(visor, env, tmp, cuenta):
    shutil.rmtree(tmp / "visor", ignore_errors=True)
    e = visor_env(env, tmp)
    t0 = time.time()
    proc = subprocess.Popen([visor], env=e, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        pagina = tmp / "boveda" / "nota.html"
        for extra in [[], ["--beside"]]:
            out = subprocess.run([visor, "open", str(pagina), *extra], env=e, capture_output=True, text=True, timeout=30)
            datos = json.loads(out.stdout)
            if not datos.get("ok") or not datos.get("loaded"):
                return {"variante": "visor", "error": out.stdout.strip()}
        apertura = (time.time() - t0) * 1000
        return medir_proceso(proc, "visor", apertura) if cuenta else None
    finally:
        parar(proc)


def una_prototipo(marco, env, tmp, cuenta):
    tiempos = tmp / f"tiempos-{marco}.txt"
    tiempos.unlink(missing_ok=True)
    t0 = time.time()
    proc = subprocess.Popen([str(BINARIO), "--marco", marco, "--abrir", PAGINA_PROTOTIPO],
                            env=dict(env, U1_TIEMPOS=str(tiempos)), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        deadline = time.time() + 20
        eventos = {}
        while not {"pagina0", "pagina1"} <= eventos.keys():
            if time.time() > deadline:
                return {"variante": marco, "error": "las páginas no cargaron en 20 s"}
            time.sleep(0.02)
            if tiempos.exists():
                eventos = {l.split()[0]: float(l.split()[2]) for l in tiempos.read_text().splitlines() if len(l.split()) == 3}
        apertura = max(eventos["pagina0"], eventos["pagina1"]) - t0 * 1000
        return medir_proceso(proc, marco, apertura) if cuenta else None
    finally:
        parar(proc)


def una(variante, visor, env, tmp, cuenta):
    if variante == "visor":
        return una_visor(visor, env, tmp, cuenta)
    return una_prototipo(variante, env, tmp, cuenta)


def main():
    args = sys.argv[1:]
    rondas = 3
    if "--rondas" in args:
        i = args.index("--rondas")
        rondas = int(args[i + 1])
        del args[i:i + 2]
    if not args:
        sys.exit(__doc__)
    visor = str(Path(args.pop(0)).resolve())
    salida = Path(args[0]) if args else Path(tempfile.gettempdir()) / f"u1-visor-{time.strftime('%Y%m%d-%H%M%S')}.jsonl"
    tmp = Path(tempfile.mkdtemp(prefix="u1-visor-"))
    # The same content as the prototype's page, in a vault of its own.
    (tmp / "boveda").mkdir()
    (tmp / "boveda" / "nota.html").write_text(
        '<!doctype html>\n<html lang="es">\n<head>\n<meta charset="utf-8">\n<title>nota 001</title>\n</head>\n'
        '<body>\n<h1>Nota 001</h1>\n<p>Texto de prueba de la carpeta-01.</p>\n</body>\n</html>\n')
    env = start_sway(tmp)
    try:
        for v in VARIANTES:
            una(v, visor, env, tmp, cuenta=False)
        for ronda in range(rondas):
            orden = VARIANTES[ronda % 3:] + VARIANTES[:ronda % 3]
            for v in orden:
                r = una(v, visor, env, tmp, cuenta=True)
                r["ronda"] = ronda + 1
                print(json.dumps(r, ensure_ascii=False), flush=True)
                with salida.open("a") as f:
                    f.write(json.dumps(r, ensure_ascii=False) + "\n")
    finally:
        subprocess.run(["systemctl", "--user", "stop", f"u1-medir-sway-{os.getpid()}"], capture_output=True)
        shutil.rmtree(tmp, ignore_errors=True)
    print(f"Resultados: {salida}")


if __name__ == "__main__":
    main()
