#!/usr/bin/python3
# Screenshots every mockup with WebKitGTK, the engine visor uses. Needs a Wayland display:
# capturar.sh starts a headless sway for it. Usage: capturar.py maqueta.html carpeta [nombre...]
import subprocess
import sys

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("WebKit2", "4.1")
from gi.repository import GLib, Gtk, WebKit2  # noqa: E402

html, carpeta, pedidos = sys.argv[1], sys.argv[2], sys.argv[3:]

casos = []
for v in "abc":
    casos += [
        (f"{v}-normal", f"v={v}&tema=actual&estado=normal"),
        (f"{v}-claro", f"v={v}&tema=claro&estado=normal"),
        (f"{v}-ajustes", f"v={v}&tema=actual&estado=ajustes"),
        (f"{v}-buscar", f"v={v}&tema=actual&estado=buscar"),
    ]
casos = [c for c in casos if not pedidos or c[0] in pedidos]

ventana = Gtk.Window(title="maquetas")
ventana.set_decorated(False)
ventana.set_default_size(1400, 900)
vista = WebKit2.WebView()
vista.get_settings().set_allow_file_access_from_file_urls(True)
ventana.add(vista)
ventana.show_all()


def siguiente():
    if not casos:
        Gtk.main_quit()
        return
    nombre, q = casos[0]
    vista.load_uri(f"file://{html}?{q}")


def capturar(nombre):
    # grim, not WebView.get_snapshot: that needs pycairo, which is not installed.
    subprocess.run(["grim", "-g", "0,0 1400x900", f"{carpeta}/{nombre}.png"], check=True)
    print(nombre, flush=True)
    casos.pop(0)
    siguiente()
    return False


def cargada(v, evento):
    if evento != WebKit2.LoadEvent.FINISHED:
        return
    nombre = casos[0][0]
    # Leave time for React and the popover's positioning to settle.
    GLib.timeout_add(700, capturar, nombre)


vista.connect("load-changed", cargada)
GLib.timeout_add(800, lambda: siguiente() and False)
GLib.timeout_add_seconds(120, Gtk.main_quit)
Gtk.main()
