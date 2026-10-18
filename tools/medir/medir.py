#!/usr/bin/env python3
"""Mide visor con una instancia aislada, según docs/pruebas.md.

    tools/medir/medir.py [--rondas N] BINARIO [BINARIO ...]
    tools/medir/medir.py --resumen RESULTADOS.jsonl

Con varios binarios alterna las rondas (A B A B …). Copia cada binario antes de
medir: cargo lo sobrescribe al compilar. Las ventanas de visor se abren en un sway
sin pantalla, así que el escritorio sigue libre; con --escritorio se abren en el
escritorio activo de Hyprland y no hay que usar el equipo mientras tanto.
"""
import argparse, atexit, json, os, shutil, signal, subprocess, sys, time
from pathlib import Path

HERE = Path(__file__).resolve().parent
WORK = Path(os.environ.get("TMPDIR", "/tmp")) / "visor-medir"
VAULT = WORK / "boveda"
LOOSE = WORK / "suelto" / "suelto.html"
SOCK = Path(os.environ["XDG_RUNTIME_DIR"]) / "visor-medir.sock"
PORT = 47999
TICK = os.sysconf("SC_CLK_TCK")

env = dict(os.environ)
env.update(
    VISOR_SOCKET=str(SOCK),
    VISOR_TIEMPOS="1",
    VISOR_NO_UPDATE="1",
    XDG_CONFIG_HOME=str(WORK / "env" / "cfg"),
    XDG_STATE_HOME=str(WORK / "env" / "state"),
    XDG_DATA_HOME=str(WORK / "env" / "data"),
    XDG_CACHE_HOME=str(WORK / "env" / "cache"),
)


def prepare():
    # A fresh copy each run: visor may write into the vault (.visor/).
    for name in ("boveda", "suelto"):
        shutil.rmtree(WORK / name, ignore_errors=True)
        shutil.copytree(HERE / name, WORK / name)


def reset_config():
    # The same vault, port and empty workspace before every cold start.
    cfg = WORK / "env" / "cfg" / "visor"
    cfg.mkdir(parents=True, exist_ok=True)
    (cfg / "config.json").write_text(json.dumps({"vaults": [str(VAULT)], "current": str(VAULT), "port": PORT}))


def cli(binary, *args, timeout=30):
    t = time.perf_counter()
    out = subprocess.run([binary, *args], env=env, capture_output=True, text=True, timeout=timeout, cwd=VAULT)
    ms = (time.perf_counter() - t) * 1000
    try:
        data = json.loads(out.stdout)
    except ValueError:
        data = {"raw": out.stdout, "err": out.stderr}
    return ms, data


def instances(binary):
    """Apps of this binary listening on the measuring socket: never the user's own visor."""
    real = os.path.realpath(binary)
    found = []
    for p in Path("/proc").iterdir():
        if not p.name.isdigit():
            continue
        try:
            if os.path.realpath(p / "exe") != real:
                continue
            environ = (p / "environ").read_bytes().split(b"\0")
            args = [a for a in (p / "cmdline").read_bytes().split(b"\0") if a]
        except OSError:
            continue
        if f"VISOR_SOCKET={SOCK}".encode() in environ and len(args) == 1:
            found.append(int(p.name))
    return found


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
    # PSS, not RSS: RSS counts shared pages once per process.
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


SWAY = WORK / "sway"
headless = True


def start_sway():
    """A sway without a screen for the windows to measure. Its window counts as
    shown, so WebKit does not slow the page down, and the desktop stays free."""
    shutil.rmtree(SWAY, ignore_errors=True)
    SWAY.mkdir(parents=True)
    display = SWAY / "display"
    (SWAY / "config").write_text(
        "output HEADLESS-1 resolution 1920x1080\n"
        "xwayland disable\n"
        f"exec sh -c 'echo $WAYLAND_DISPLAY > {display}'\n")
    # As a user service of its own: started as a child of some shells (an agent's),
    # sway gets SIGKILL within milliseconds, even in its own scope.
    unit = "visor-medir-sway"
    subprocess.run(["systemctl", "--user", "stop", unit], capture_output=True)
    subprocess.run(["systemd-run", "--user", "--quiet", "--collect", f"--unit={unit}",
                    "-E", "WLR_BACKENDS=headless", "-E", "WLR_LIBINPUT_NO_DEVICES=1",
                    "-E", f"SWAYSOCK={SWAY / 'ipc.sock'}", "-E", f"XDG_RUNTIME_DIR={os.environ['XDG_RUNTIME_DIR']}",
                    "sway", "-c", str(SWAY / "config")], check=True)
    atexit.register(subprocess.run, ["systemctl", "--user", "stop", unit], capture_output=True)
    deadline = time.time() + 10
    while not (display.exists() and display.read_text().strip()):
        assert time.time() < deadline, f"sway no arrancó; mira journalctl --user -u {unit}"
        time.sleep(0.1)
    env.update(WAYLAND_DISPLAY=display.read_text().strip(), SWAYSOCK=str(SWAY / "ipc.sock"))
    env.pop("DISPLAY", None)


def window(pid):
    """Address of the app's window and whether it is on the active workspace:
    WebKit slows hidden pages down, which would flatter the idle CPU."""
    if headless:
        tree = json.loads(subprocess.run(["swaymsg", "-t", "get_tree"], env=env, capture_output=True, text=True).stdout)
        nodes = [tree]
        while nodes:
            n = nodes.pop()
            if n.get("pid") == pid:
                return n["id"], n["visible"]
            nodes += n.get("nodes", []) + n.get("floating_nodes", [])
        return None, False
    clients = json.loads(subprocess.run(["hyprctl", "clients", "-j"], capture_output=True, text=True).stdout)
    active = json.loads(subprocess.run(["hyprctl", "activeworkspace", "-j"], capture_output=True, text=True).stdout)["id"]
    for c in clients:
        if c["pid"] == pid:
            return c["address"], c["workspace"]["id"] == active
    return None, False


def key(address, mods, k):
    if headless:
        # The window has the focus: it is the only one. wtype's first key gets lost
        # while sway takes its keymap, so a key nobody uses goes first.
        subprocess.run(["wtype", "-P", "F24", "-p", "F24", "-s", "100", "-M", mods.lower(), "-P", k.lower(),
                        "-p", k.lower(), "-m", mods.lower()], env=env, capture_output=True)
        return
    for state in ("down", "up"):
        subprocess.run(["hyprctl", "dispatch",
                        f'hl.dsp.send_key_state({{ mods = "{mods}", key = "{k}", state = "{state}", window = "address:{address}" }})'],
                       capture_output=True)


def close(binary):
    for pid in instances(binary):
        os.kill(pid, signal.SIGTERM)
    deadline = time.time() + 10
    while instances(binary) and time.time() < deadline:
        time.sleep(0.1)
    for pid in instances(binary):
        os.kill(pid, signal.SIGKILL)
    time.sleep(1.5)  # WebKit's processes outlive the app for a moment


def cold(binary, target):
    close(binary)
    reset_config()
    ms, data = cli(binary, "open", str(target))
    assert data.get("ok"), data
    found = instances(binary)
    assert len(found) == 1, f"Se esperaba una instancia de medida y hay {found}"
    return round(ms), found[0]


def idle_cpu(pid, seconds=30):
    before = cpu_ticks(pid)
    time.sleep(seconds)
    ticks = cpu_ticks(pid) - before
    return ticks, round(ticks / TICK / seconds * 100, 3)


def measure(binary):
    r = {"binario": Path(binary).name, "hora": time.strftime("%H:%M:%S")}

    # Minimal note: cold start, memory, idle CPU, then page change and new tab.
    r["arranque_minima_ms"], pid = cold(binary, VAULT / "minima.md")
    time.sleep(10)
    r["memoria_minima_mb"] = pss_mb(pid)
    visible = window(pid)[1]
    r["cpu_minima_ticks"], r["cpu_minima_pct"] = idle_cpu(pid)
    r["cpu_minima_visible"] = visible and window(pid)[1]
    # Inside the app (VISOR_TIEMPOS) and as an agent sees it, CLI start included.
    ms, data = cli(binary, "open", "otra.md")
    r["cambio_pagina_ms"], r["cambio_pagina_cli_ms"] = data.get("ms"), round(ms)
    ms, data = cli(binary, "open", "--tab", "tareas.md")
    r["pestana_nueva_ms"], r["pestana_nueva_cli_ms"] = data.get("ms"), round(ms)

    # Editor (Ctrl+E) on the minimal note, from a fresh start.
    _, pid = cold(binary, VAULT / "minima.md")
    time.sleep(5)
    key(window(pid)[0], "CTRL", "E")
    time.sleep(10)
    r["memoria_editor_mb"] = pss_mb(pid)

    # Heavy note: Mermaid, KaTeX and code.
    r["arranque_ondas_ms"], pid = cold(binary, VAULT / "estudio" / "física" / "ondas.md")
    time.sleep(10)
    r["memoria_ondas_mb"] = pss_mb(pid)
    visible = window(pid)[1]
    r["cpu_ondas_ticks"], r["cpu_ondas_pct"] = idle_cpu(pid)
    r["cpu_ondas_visible"] = visible and window(pid)[1]

    # A PDF of the vault, in PDF.js's full viewer (ADR 0037). Binaries older
    # than #37 do not open PDFs: their row leaves these empty.
    try:
        r["arranque_pdf_ms"], pid = cold(binary, VAULT / "estudio" / "física" / "ondas-apuntes.pdf")
    except AssertionError:
        r.update(arranque_pdf_ms=None, memoria_pdf_mb=None, cpu_pdf_ticks=None, cpu_pdf_pct=None, cpu_pdf_visible=False)
    else:
        time.sleep(10)
        r["memoria_pdf_mb"] = pss_mb(pid)
        visible = window(pid)[1]
        r["cpu_pdf_ticks"], r["cpu_pdf_pct"] = idle_cpu(pid)
        r["cpu_pdf_visible"] = visible and window(pid)[1]

    # An HTML page outside every vault.
    r["arranque_suelto_ms"], pid = cold(binary, LOOSE)
    time.sleep(10)
    r["memoria_suelto_mb"] = pss_mb(pid)

    # Three.js: the page logs its own frame rate once a second.
    _, pid = cold(binary, VAULT / "threejs.html")
    cli(binary, "console", "--clear")
    time.sleep(8)
    log = cli(binary, "console")[1].get("console", [])
    r["fps"] = [int(e["text"].split()[1]) for e in log if str(e.get("text", "")).startswith("FPS ")][-5:]
    r["fps_visible"] = window(pid)[1]
    close(binary)
    return r


def summary(path):
    rows = [json.loads(line) for line in open(path)]
    binaries = list(dict.fromkeys(r["binario"] for r in rows))
    keys = [k for k in rows[0] if k not in ("binario", "hora", "ronda") and not k.endswith("visible")]
    print(f"{'medida':22}" + "".join(f"{b:>26}" for b in binaries))
    for k in keys:
        cells = []
        for b in binaries:
            mine = [r for r in rows if r["binario"] == b]
            if k.startswith("cpu_"):
                # Rounds where the window was hidden or someone used the machine do not count.
                page = k.split("_")[1]
                mine = [r for r in mine if r[f"cpu_{page}_visible"]]
            # A binary without VISOR_TIEMPOS leaves the in-app times empty.
            values = [min(r[k]) if k == "fps" and r[k] else (0 if k == "fps" else r.get(k)) for r in mine]
            values = sorted(v for v in values if v is not None)
            cells.append(f"{values[len(values) // 2]} ({values[0]}–{values[-1]})" if values else "—")
        print(f"{k:22}" + "".join(f"{c:>26}" for c in cells))


def main():
    parser = argparse.ArgumentParser(description="Mide arranque, memoria, CPU en reposo y fps de visor.")
    parser.add_argument("binarios", nargs="*")
    parser.add_argument("--rondas", type=int, default=1)
    parser.add_argument("--resumen", metavar="RESULTADOS")
    parser.add_argument("--escritorio", action="store_true", help="medir en el escritorio de Hyprland, no en un sway sin pantalla")
    args = parser.parse_args()
    if args.resumen:
        return summary(args.resumen)
    if not args.binarios:
        parser.error("indica al menos un binario")
    binaries = [os.path.abspath(b) for b in args.binarios]
    global headless
    headless = not args.escritorio
    if headless:
        start_sway()
    prepare()
    out_path = WORK / time.strftime("resultados-%Y%m%d-%H%M.jsonl")
    print(f"Resultados en {out_path}", file=sys.stderr)
    with open(out_path, "w") as out:
        for i in range(args.rondas):
            for b in binaries:
                r = measure(b)
                r["ronda"] = i + 1
                print(json.dumps(r, ensure_ascii=False), flush=True)
                out.write(json.dumps(r, ensure_ascii=False) + "\n")
                out.flush()
    summary(out_path)


if __name__ == "__main__":
    main()
