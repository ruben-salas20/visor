#!/usr/bin/env python3
"""Compare visor with Obsidian and Chromium on this machine, with medir.py's method.

    tools/medir/comparar.py [--rondas N] [--binario RUTA]   full run (JSONL in WORK)
    tools/medir/comparar.py --prueba [--binario RUTA]       one short measure per app, with screenshots
    tools/medir/comparar.py --resumen RESULTADOS.jsonl      table and docs/img/cifras.json

visor detaches from its CLI, so it is identified by its own socket in its
environment. Obsidian and Chromium rewrite their environment when they set the
process title, so they are identified by the window's ancestry and by the
unique --user-data-dir of each launch. Each app keeps one profile for the whole run,
warmed up by a discarded launch, as visor keeps its caches.
"""
import argparse, json, os, re, secrets, shutil, signal, subprocess, sys, threading, time
from datetime import date
from pathlib import Path
from urllib.parse import quote

import medir
from medir import HERE, LOOSE, VAULT, WORK, cli, close as close_visor, idle_cpu, pss_mb, prepare, reset_config, window

REPO = HERE.parent.parent
VISOR_BIN = WORK / "visor-bin"  # cargo overwrites target/release/visor; measure a copy
OBSIDIAN_APP = Path("/usr/lib/obsidian/app.asar")  # what /usr/bin/obsidian passes to electron
OBSIDIAN_USER_ASAR = Path.home() / ".config/obsidian/obsidian-1.14.4.asar"  # read only: copied, never touched
MIB = 1024 * 1024
LOADED_ASAR = re.compile(rb"Loaded updated app \S*obsidian-(\d+\.\d+\.\d+)\.asar")


def casos():
    # Apps that open each case, in the order they run inside a round.
    return {
        "minima": (VAULT / "minima.md", ["visor", "obsidian"]),
        "ondas": (VAULT / "estudio" / "física" / "ondas.md", ["visor", "obsidian"]),
        "suelto": (LOOSE, ["visor", "chromium"]),
    }


def send(pid, sig):
    try:
        os.kill(pid, sig)
    except ProcessLookupError:
        pass


def leer(path):
    try:
        return Path(path).read_text().strip()
    except OSError:
        return ""


def environ_of(pid):
    try:
        return Path(f"/proc/{pid}/environ").read_bytes().split(b"\0")
    except OSError:
        return []


def owned_visor(pid, binary):
    """Only the app of this copy on our socket: the CLI has the same exe but more arguments."""
    try:
        exe = os.path.realpath(f"/proc/{pid}/exe")
        args = [a for a in Path(f"/proc/{pid}/cmdline").read_bytes().split(b"\0") if a]
    except OSError:
        return False
    return exe == os.path.realpath(binary) and len(args) == 1 and f"VISOR_SOCKET={medir.SOCK}".encode() in environ_of(pid)


def ppid_of(pid):
    try:
        return int((Path("/proc") / str(pid) / "stat").read_text().rsplit(")", 1)[1].split()[1])
    except (OSError, IndexError, ValueError):
        return 0


def descends_from(pid, root):
    while pid > 1:
        if pid == root:
            return True
        pid = ppid_of(pid)
    return False


def run_pids(root, prof):
    """Processes of one Chromium or Electron launch, while any of them is alive.

    The main process's tree, plus any process that still names this launch's
    profile: children that were re-parented when the main process died.
    """
    pids = set()
    if root and Path(f"/proc/{root}").exists():
        pids.update(medir.tree(root))
    needle = f"--user-data-dir={prof}".encode()
    for p in Path("/proc").iterdir():
        if p.name.isdigit():
            try:
                if needle in (p / "cmdline").read_bytes():
                    pids.add(int(p.name))
            except OSError:
                pass
    return sorted(pids)


def hypr_clients():
    return json.loads(subprocess.run(["hyprctl", "clients", "-j"], capture_output=True, text=True).stdout)


def client_of(pid):
    return next((c for c in hypr_clients() if c["pid"] == pid), None)


def wait_window(owned, t0, timeout=30):
    """Polls Hyprland until a window of our launch appears. Returns (pid, ms since t0)."""
    checked = {}
    end = time.perf_counter() + timeout
    while time.perf_counter() < end:
        for c in hypr_clients():
            pid = c["pid"]
            if pid not in checked:
                checked[pid] = owned(pid)
            if checked[pid]:
                return pid, (time.perf_counter() - t0) * 1000
        time.sleep(0.01)
    raise RuntimeError("ninguna ventana de la medida apareció en 30 s")


def app_env(root):
    env = dict(os.environ)
    # Own XDG dirs: no user config is read. The Arch launchers read <XDG_CONFIG_HOME>/…-flags.conf.
    for name, sub in (("XDG_CONFIG_HOME", "cfg"), ("XDG_STATE_HOME", "state"),
                      ("XDG_DATA_HOME", "data"), ("XDG_CACHE_HOME", "cache")):
        env[name] = str(root / sub)
    return env


def start(app, target, binary, root, holder):
    """Launches the app the way its own launcher does. Returns (window pid, arranque ms, CLI ms).

    holder["proc"] gets the Popen as soon as it exists, so the caller can close it
    even when the window never shows up.
    """
    if app == "visor":
        close_visor(binary)
        reset_config()
        t0 = time.perf_counter()
        box = {}

        def open_page():
            box["ms"], box["data"] = cli(binary, "open", str(target))

        th = threading.Thread(target=open_page)
        th.start()
        pid, ms = wait_window(lambda p: owned_visor(p, binary), t0)
        th.join()
        assert box["data"].get("ok"), box["data"]
        return pid, round(ms), round(box["ms"])

    prof = root / "perfil"
    if app == "obsidian":
        if not prof.exists():
            prof.mkdir(parents=True)
            # Obsidian loads <userData>/obsidian-<version>.asar when it exists, so this copy is the 1.14.4 build.
            shutil.copy2(OBSIDIAN_USER_ASAR, prof / OBSIDIAN_USER_ASAR.name)
            vault = {"path": str(VAULT), "ts": int(time.time() * 1000), "open": True}
            (prof / "obsidian.json").write_text(json.dumps({"vaults": {secrets.token_hex(8): vault}}))
        cmd = ["electron43", str(OBSIDIAN_APP), f"--user-data-dir={prof}",
               "obsidian://open?path=" + quote(str(target), safe="")]
    else:
        cmd = ["chromium", f"--user-data-dir={prof}", "--no-first-run", "--no-default-browser-check",
               "--hide-crash-restore-bubble", "--new-window", target.as_uri()]
    t0 = time.perf_counter()
    proc = subprocess.Popen(cmd, env=app_env(root), stdin=subprocess.DEVNULL,
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    holder["proc"] = proc
    pid, ms = wait_window(lambda p: descends_from(p, proc.pid), t0)
    return pid, round(ms), None


def close_run(prof, proc):
    """Closes every process of one launch by PID: SIGTERM, wait, SIGKILL."""
    def alive():
        if proc:
            proc.poll()  # reaps the main process, so it stops showing up as a zombie
        return run_pids(proc.pid if proc else None, prof)

    # Only the app's own PID gets SIGTERM. Signalling every helper at once made Obsidian's
    # main process crash (SIGTRAP, core dump) on each close.
    if proc:
        send(proc.pid, signal.SIGTERM)
    deadline = time.time() + 10
    while time.time() < deadline and alive():
        time.sleep(0.1)
    for pid in alive():
        send(pid, signal.SIGKILL)
    if proc:
        proc.wait()
    time.sleep(1.5)  # as medir.close: the browser's helpers outlive the main process for a moment


def close(app, binary, root, proc):
    if app == "visor":
        close_visor(binary)
    else:
        close_run(root / "perfil", proc)


def obsidian_version(prof):
    # The log names the asar that was loaded, so the number is what ran, not what is installed.
    try:
        found = LOADED_ASAR.findall((prof / "obsidian.log").read_bytes())
    except OSError:
        found = []
    return found[-1].decode() if found else "desconocida"


def capture(client, png):
    x, y = client["at"]
    w, h = client["size"]
    subprocess.run(["grim", "-g", f"{x},{y} {w}x{h}", str(png)], check=True)


def run_app(app, target, binary, versions, settle, cpu_seconds=0, shot=None):
    """One cold start of `app` on `target`, closed again before returning."""
    root = WORK / "env" / app
    holder = {}
    try:
        pid, arranque, carga = start(app, target, binary, root, holder)
        time.sleep(settle)
        r = {"arranque_ms": arranque}
        if carga is not None:
            r["arranque_carga_ms"] = carga
        r["memoria_mb"] = pss_mb(pid)
        if app == "visor":
            r["version"] = versions["visor"]
        elif app == "obsidian":
            r["version"] = obsidian_version(root / "perfil")
        else:
            r["version"] = versions["chromium"]
        if cpu_seconds:
            visible = window(pid)[1]
            _, r["cpu_pct"] = idle_cpu(pid, cpu_seconds)
            r["cpu_visible"] = bool(visible and window(pid)[1])
        if shot:
            client = client_of(pid)
            r["visible"] = window(pid)[1]
            r["xwayland"] = client.get("xwayland")
            if r["visible"]:
                capture(client, shot)
                r["captura"] = str(shot)
        return r
    finally:
        close(app, binary, root, holder.get("proc"))


def git_commit():
    return subprocess.run(["git", "-C", str(REPO), "rev-parse", "--short", "HEAD"],
                          capture_output=True, text=True, check=True).stdout.strip()


def chromium_version():
    out = subprocess.run(["chromium", "--version"], capture_output=True, text=True).stdout
    found = re.search(r"Chromium (\S+)", out)
    return found.group(1) if found else "desconocida"


def prueba(binary, versions):
    fresh_profiles()
    shots = WORK / "prueba"
    shots.mkdir(parents=True, exist_ok=True)
    for caso, (target, apps) in casos().items():
        for app in apps:
            r = run_app(app, target, binary, versions, settle=8, shot=shots / f"{app}-{caso}.png")
            print(json.dumps({"caso": caso, "app": app, **r}, ensure_ascii=False), flush=True)


def fresh_profiles():
    for app in ("obsidian", "chromium"):
        shutil.rmtree(WORK / "env" / app, ignore_errors=True)


def full(binary, versions, rondas, out_path):
    fresh_profiles()
    # First launches build caches and profiles; nobody measures an app's very first start.
    for caso, (target, apps) in casos().items():
        for app in apps:
            run_app(app, target, binary, versions, settle=3)
    with open(out_path, "w", encoding="utf-8") as out:
        for i in range(rondas):
            for caso, (target, apps) in casos().items():
                # Alternating the order removes any advantage of going first.
                for app in (apps if i % 2 == 0 else apps[::-1]):
                    hora = time.strftime("%H:%M:%S")
                    r = run_app(app, target, binary, versions, settle=10, cpu_seconds=30)
                    line = {"app": app, "version": r.pop("version"), "caso": caso, "ronda": i + 1, "hora": hora}
                    line.update(r)
                    if app == "visor":
                        line["binario_mb"] = round(os.path.getsize(binary) / MIB, 3)
                    text = json.dumps(line, ensure_ascii=False)
                    print(text, flush=True)
                    out.write(text + "\n")
                    out.flush()


def stats(rows, metric):
    mine = [r for r in rows if metric in r]
    if metric == "cpu_pct":
        # Rounds where the window was hidden or the machine was busy do not count, as in medir.py.
        mine = [r for r in mine if r["cpu_visible"]]
    values = sorted(r[metric] for r in mine if r[metric] is not None)
    if not values:
        return None
    return {"mediana": values[len(values) // 2], "min": values[0], "max": values[-1]}


def installed_mib(package):
    out = subprocess.run(["pacman", "-Qi", package], capture_output=True, text=True, env={**os.environ, "LANG": "C"}).stdout
    found = re.search(r"Installed Size\s*:\s*([\d.]+) (KiB|MiB|GiB)", out)
    factor = {"KiB": 1 / 1024, "MiB": 1, "GiB": 1024}
    return float(found.group(1)) * factor[found.group(2)] if found else 0.0


def conditions(rondas):
    mains = [d for d in Path("/sys/class/power_supply").iterdir() if leer(d / "type") == "Mains"]
    enchufado = any(leer(d / "online") == "1" for d in mains)
    perfil = subprocess.run(["powerprofilesctl", "get"], capture_output=True, text=True).stdout.strip() or "desconocido"
    cpu = next((l.split(":", 1)[1].strip() for l in Path("/proc/cpuinfo").read_text().splitlines()
                if l.startswith("model name")), "CPU desconocida")
    ram_kb = next(int(l.split()[1]) for l in Path("/proc/meminfo").read_text().splitlines() if l.startswith("MemTotal"))
    return (f"{'enchufado' if enchufado else 'con batería'}, perfil {perfil}, {rondas} rondas, "
            f"{cpu}, {ram_kb / 1024 / 1024:.1f} GiB")


def summary(path):
    rows = [json.loads(line) for line in open(path, encoding="utf-8")]
    combos = list(dict.fromkeys((r["app"], r["caso"]) for r in rows))
    metrics = ["arranque_ms", "arranque_carga_ms", "memoria_mb", "cpu_pct"]

    print(f"{'medida':22}" + "".join(f"{a + ' ' + c:>24}" for a, c in combos))
    for m in metrics:
        cells = []
        for a, c in combos:
            s = stats([r for r in rows if r["app"] == a and r["caso"] == c], m)
            cells.append(f"{s['mediana']} ({s['min']}–{s['max']})" if s else "—")
        print(f"{m:22}" + "".join(f"{x:>24}" for x in cells))

    medidas = {}
    for a, c in combos:
        sub = [r for r in rows if r["app"] == a and r["caso"] == c]
        medidas.setdefault(a, {})[c] = {m: stats(sub, m) for m in metrics if any(m in r for r in sub)}

    versiones = {}
    for r in rows:
        versiones.setdefault(r["app"], r["version"])
    visor_bin = next((r["binario_mb"] for r in rows if r["app"] == "visor"), None)

    obs_app = installed_mib("obsidian")
    if versiones.get("obsidian") == "1.14.4" and OBSIDIAN_USER_ASAR.exists():
        # The asar is the app that runs; the packaged one stays installed but unused.
        obs_app += (OBSIDIAN_USER_ASAR.stat().st_size - (OBSIDIAN_APP.parent / "obsidian.asar").stat().st_size) / MIB
    tamano = {
        "visor": {"app": round(visor_bin, 1) if visor_bin else None,
                  "motor": round(installed_mib("webkit2gtk-4.1"), 1),
                  "motor_nombre": "webkit2gtk-4.1 (compartido del sistema)"},
        "obsidian": {"app": round(obs_app, 1), "motor": round(installed_mib("electron43"), 1),
                     "motor_nombre": "electron43"},
        "chromium": {"app": round(installed_mib("chromium"), 1), "motor": 0, "motor_nombre": ""},
    }

    found = re.search(r"(\d{4})(\d{2})(\d{2})-\d{4}", Path(path).name)
    fecha = f"{found.group(1)}-{found.group(2)}-{found.group(3)}" if found else date.today().isoformat()
    cifras = {"fecha": fecha, "condiciones": conditions(len({r["ronda"] for r in rows})),
              "versiones": versiones, "medidas": medidas, "tamano_mb": tamano}
    out = REPO / "docs" / "img" / "cifras.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(cifras, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"\nEscrito {out}")


def main():
    parser = argparse.ArgumentParser(description="Compara visor con Obsidian y Chromium: arranque, memoria y CPU en reposo.")
    parser.add_argument("--rondas", type=int, default=5)
    parser.add_argument("--binario", default=str(REPO / "target/release/visor"))
    parser.add_argument("--prueba", action="store_true", help="una medida corta por app, con capturas")
    parser.add_argument("--resumen", metavar="RESULTADOS")
    args = parser.parse_args()
    if args.resumen:
        return summary(args.resumen)

    prepare()
    shutil.copy2(args.binario, VISOR_BIN)
    binary = str(VISOR_BIN)
    versions = {"visor": git_commit(), "chromium": chromium_version()}
    if args.prueba:
        return prueba(binary, versions)
    out_path = WORK / time.strftime("comparar-%Y%m%d-%H%M.jsonl")
    print(f"Resultados en {out_path}", file=sys.stderr)
    full(binary, versions, args.rondas, out_path)
    print(f"Tabla y docs/img/cifras.json: comparar.py --resumen {out_path}", file=sys.stderr)


if __name__ == "__main__":
    main()
