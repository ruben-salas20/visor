#!/usr/bin/env python3
"""Draws the README charts (visor vs Obsidian and Chromium) as hand-written SVG.

    tools/medir/graficas.py [--datos RUTA] [--salida CARPETA]

Reads docs/img/cifras.json, or the example file when that one is missing (with a
warning on stderr), and writes <nombre>.es.svg and <nombre>.en.svg into
docs/img/graficas/. Standard library only. Light and dark colors live in one
<style> block with @media (prefers-color-scheme: dark): GitHub shows SVG through
<img> and follows the viewer's theme.
"""
from decimal import ROUND_HALF_UP, Decimal
import argparse
import json
import math
import re
import sys
from pathlib import Path
from xml.sax.saxutils import escape

RAIZ = Path(__file__).resolve().parents[2]
IMG = RAIZ / "docs" / "img"
DATOS_POR_DEFECTO = IMG / "cifras.json"
EJEMPLO = IMG / "cifras.ejemplo.json"
SALIDA_POR_DEFECTO = IMG / "graficas"

ANCHO = 720
MARGEN = 24
# Blue is categorical slot 1 of the reference palette (docs/dataviz palette.md),
# the rest are neutrals. The other app's engine is the neutral step that clears 3:1
# against its surface (darker on light, lighter on dark). The texture uses
# tone-on-tone steps of the same ramp.
COLORES = {
    "claro": dict(fondo="#fcfcfb", texto="#0b0b0b", texto2="#52514e", rejilla="#e1e0d9",
                  eje="#c3c2b7", acento="#2a78d6", acento_fondo="#9ec5f4", acento_trama="#1c5cab",
                  neutro="#898781", neutro_claro="#52514e", borde="#e1e0d9"),
    "oscuro": dict(fondo="#1a1a19", texto="#ffffff", texto2="#c3c2b7", rejilla="#2c2c2a",
                   eje="#383835", acento="#3987e5", acento_fondo="#184f95", acento_trama="#86b6ef",
                   neutro="#898781", neutro_claro="#c3c2b7", borde="#2c2c2a"),
}
FUENTE = 'system-ui, -apple-system, "Segoe UI", sans-serif'
# Below this a ratio is shown as a plain number, never as "N× less".
UMBRAL_PRESUMIR = 1.2

CASOS_BIGOTE = [
    ("minima", ["visor", "obsidian"]),
    ("ondas", ["visor", "obsidian"]),
    ("suelto", ["visor", "chromium"]),
]


# Helvetica advance widths (1/1000 em). Fonts differ by viewer (GitHub shows the
# reader's system-ui), so the estimate is padded to cover wider faces like Segoe UI or Inter.
_ANCHOS = {**dict.fromkeys("ijl|!.,:;'·", 278), **dict.fromkeys("ftI []/", 278),
           **dict.fromkeys("r()-", 333), **dict.fromkeys("ckvxyzsJ«»“”", 500),
           "m": 833, "w": 722, "M": 833, "W": 944, "%": 889, "×": 584, "–": 556, "—": 1000}
_HOLGURA = 1.12


def ancho(s, size, weight=None):
    """Estimated width in px of `s`, on the wide side."""
    em = sum(_ANCHOS.get(ch, 722 if ch.isupper() else 556) for ch in s) / 1000
    return em * size * _HOLGURA * (1.06 if weight and int(weight) >= 600 else 1.0)


def envolver(s, size, max_px, weight=None):
    """Splits `s` into lines that fit `max_px`, breaking only between words."""
    lineas, actual = [], ""
    for palabra in s.split(" "):
        prueba = f"{actual} {palabra}".strip()
        if actual and ancho(prueba, size, weight) > max_px:
            lineas.append(actual)
            actual = palabra
        else:
            actual = prueba
    lineas.append(actual)
    return lineas


def equilibrar(s, size, max_px, weight=None):
    """Like envolver, but with lines of similar length: no lone word on the last line."""
    lineas = envolver(s, size, max_px, weight)
    n, lo, hi = len(lineas), 0.0, max_px
    while n > 1 and hi - lo > 2:
        mid = (lo + hi) / 2
        if len(envolver(s, size, mid, weight)) == n:
            hi = mid
        else:
            lo = mid
    return envolver(s, size, hi, weight) if n > 1 else lineas


# ---------------------------------------------------------------- data access

def cargar(ruta):
    try:
        return json.loads(Path(ruta).read_text(encoding="utf-8"))
    except (OSError, ValueError) as e:
        sys.exit(f"graficas: no puedo leer {ruta}: {e}")


def medida(datos, app, caso, metrica):
    try:
        return datos["medidas"][app][caso][metrica]
    except KeyError as e:
        sys.exit(f"graficas: falta medidas.{app}.{caso}.{metrica} ({e})")


def tamano(datos, app):
    try:
        return datos["tamano_mb"][app]
    except KeyError as e:
        sys.exit(f"graficas: falta tamano_mb.{app} ({e})")


def ratio(datos, otra, caso, metrica):
    """How many times more the other app uses, on medians (visor = 1)."""
    v = medida(datos, "visor", caso, metrica)["mediana"]
    o = medida(datos, otra, caso, metrica)["mediana"]
    return o / v


# ------------------------------------------------------------------ formatting

def numero(x, dec, lang):
    # Half up, like the intro animation's Math.round: format() rounds 412.5 to 412.
    x = Decimal(str(x)).quantize(Decimal(1).scaleb(-dec), ROUND_HALF_UP)
    s = f"{x:,.{dec}f}"  # en: 1,450.0
    if lang == "es":
        # decimal comma; thousands separated by a no-break space
        s = s.replace(",", "\0").replace(".", ",").replace("\0", " ")
    return s


def veces(r, lang):
    return numero(r, 1, lang) + "×"


def textos(lang):
    es = lang == "es"
    return dict(
        datos="Datos del" if es else "Data of",
        versiones="Versiones:" if es else "Versions:",
        ejemplo=("EJEMPLO: cifras inventadas, no medidas." if es
                 else "EXAMPLE: invented figures, not measured."),
        visor="visor",
        obsidian="Obsidian",
        chromium="Chromium",
        casos={
            "minima": "Nota de un párrafo" if es else "One-paragraph note",
            "ondas": "Nota con Mermaid, KaTeX y código" if es else "Note with Mermaid, KaTeX and code",
            "suelto": "Página HTML suelta" if es else "Loose HTML page",
        },
        leyenda_otra="Obsidian o Chromium" if es else "Obsidian or Chromium",
        bigote=("Barra: mediana. Línea fina: mínimo–máximo de las rondas."
                if es else "Bar: median. Thin line: min–max across the rounds."),
    )


def paso_eje(vmax):
    """Round tick step giving about 4-6 ticks, and the axis top that covers vmax."""
    bruto = vmax / 5
    mag = 10 ** math.floor(math.log10(bruto))
    paso = next(m * mag for m in (1, 2, 2.5, 5, 10) if m * mag >= bruto)
    top = math.ceil(vmax / paso) * paso
    return paso, top


def condiciones(datos, t):
    # "13th Gen Intel(R) Core(TM) i7-13620H" → "Intel Core i7-13620H": the marks only take room.
    texto = re.sub(r"\d+th Gen |\(R\)|\(TM\)", "", datos["condiciones"])
    # cifras.json is written in Spanish; the English charts translate its fixed words.
    if t["datos"] == "Datos del":
        return re.sub(r"(\d)\.(\d)", r"\1,\2", texto)
    for es, en in (("enchufado", "plugged in"), ("perfil performance", "performance profile"), ("rondas", "rounds")):
        texto = texto.replace(es, en)
    return texto


def pie_lineas(datos, t):
    v = datos["versiones"]
    lineas = [
        f"{t['datos']} {datos['fecha']} · {condiciones(datos, t)}",
        f"{t['versiones']} visor {v['visor']} · Obsidian {v['obsidian']} · Chromium {v['chromium']}",
    ]
    if datos.get("ejemplo"):
        lineas.append(t["ejemplo"])
    return lineas


# --------------------------------------------------------------------- SVG kit

def cabe(fin, hasta, s):
    # Fail the build rather than ship a label that spills out of its box.
    if fin > hasta:
        raise ValueError(f"el texto no cabe ({fin:.0f} > {hasta:.0f} px): {s!r}")


class Lienzo:
    def __init__(self):
        self.partes = []
        self.cabeza = []
        self.dy = 0  # extra height of a wrapped title; the body moves down by it
        self.alto = 0

    def add(self, s):
        self.partes.append(s)

    def texto(self, x, y, s, clase="t1", size=13, weight=None, anchor="start", knock=False, hasta=ANCHO - MARGEN):
        if anchor == "start":
            cabe(x + ancho(s, size, weight), hasta, s)
        w = f' font-weight="{weight}"' if weight else ""
        a = f' text-anchor="{anchor}"' if anchor != "start" else ""
        k = " knock" if knock else ""
        self.add(f'<text x="{x:.1f}" y="{y:.1f}" class="{clase}{k}" font-size="{size}"{w}{a}>{escape(s)}</text>')

    def cabecera(self, titulo, subtitulo):
        cuerpo, self.partes = self.partes, self.cabeza
        y = 32
        for linea in equilibrar(titulo, 18, ANCHO - 2 * MARGEN, 600):
            self.texto(MARGEN, y, linea, size=18, weight=600)
            y += 24
        for linea in equilibrar(subtitulo, 13, ANCHO - 2 * MARGEN):
            self.texto(MARGEN, y - 2, linea, clase="t2", size=13)
            y += 18
        self.dy = y - 74
        self.partes = cuerpo

    def leyenda(self, x, y, items):
        """items: (clase, texto); clase 'trama' draws the hatch swatch."""
        for clase, label in items:
            fill = ' fill="url(#trama)"' if clase == "trama" else ""
            cl = "" if clase == "trama" else f' class="{clase}"'
            self.add(f'<rect x="{x}" y="{y - 10}" width="14" height="14" rx="2"{cl}{fill}/>')
            self.texto(x + 20, y + 1, label, clase="t2", size=12)
            x += 20 + ancho(label, 12) + 22

    def pie(self, y, lineas):
        self.add(f'<line x1="{MARGEN}" y1="{y - 14}" x2="{ANCHO - MARGEN}" y2="{y - 14}" class="rej"/>')
        lineas = [l for s in lineas for l in equilibrar(s, 12, ANCHO - 2 * MARGEN)]
        for i, s in enumerate(lineas):
            self.texto(MARGEN, y + i * 17, s, clase="t2", size=12)
        self.alto = y + len(lineas) * 17 + 14

    def eje_mb(self, x0, x1, px, top, paso, y_ini, y_eje, unidad):
        """Gridlines, baseline and ticks. Returns the SVG items; the caller
        inserts them before the marks so gridlines never paint over bars."""
        grid = Lienzo()
        grid.lang = self.lang
        v = 0.0
        while v <= top + 1e-9:
            xx = px(v)
            grid.add(f'<line x1="{xx:.1f}" y1="{y_ini}" x2="{xx:.1f}" y2="{y_eje:.1f}" class="rej"/>')
            grid.texto(xx, y_eje + 16, numero(v, 0, self.lang), clase="t2", size=12, anchor="middle")
            v += paso
        grid.add(f'<line x1="{x0}" y1="{y_eje:.1f}" x2="{x1}" y2="{y_eje:.1f}" class="eje"/>')
        grid.texto(x0, y_eje + 34, unidad, clase="t2", size=12)
        return grid.partes

    def render(self, titulo, descripcion):
        alto = self.alto + self.dy
        cabecera = (
            f'<svg xmlns="http://www.w3.org/2000/svg" width="{ANCHO}" height="{alto:.0f}" '
            f'viewBox="0 0 {ANCHO} {alto:.0f}" role="img" aria-labelledby="t d">\n'
            f'<title id="t">{escape(titulo)}</title>\n<desc id="d">{escape(descripcion)}</desc>\n'
        )
        fondo = f'<rect class="fondo" x="0" y="0" width="{ANCHO}" height="{alto:.0f}" rx="8"/>\n'
        cuerpo = f'<g transform="translate(0,{self.dy})">\n' + "\n".join(self.partes) + "\n</g>"
        return cabecera + _estilo() + fondo + "\n".join(self.cabeza) + "\n" + cuerpo + "\n</svg>\n"


def _reglas(c):
    return (
        f".fondo{{fill:{c['fondo']}}}"
        f".t1{{fill:{c['texto']}}}.t2{{fill:{c['texto2']}}}"
        f".rej{{stroke:{c['rejilla']};stroke-width:1}}"
        f".eje{{stroke:{c['eje']};stroke-width:1}}"
        f".acento{{fill:{c['acento']}}}"
        f".neutro{{fill:{c['neutro']}}}.neutro-claro{{fill:{c['neutro_claro']}}}"
        f".trama{{stroke:{c['acento_trama']}}}.trama-fondo{{fill:{c['acento_fondo']}}}"
        f".bigote{{stroke:{c['texto']};stroke-width:1.5}}"
        f".halo{{stroke:{c['fondo']};stroke-width:5}}"
        f".knock{{stroke:{c['fondo']};stroke-width:4px;stroke-linejoin:round;paint-order:stroke}}"
        f".tarjeta{{fill:none;stroke:{c['borde']};stroke-width:1}}"
    )


def _estilo():
    return (
        "<style>\n"
        f"text{{font-family:{FUENTE}}}\n"
        f"{_reglas(COLORES['claro'])}\n"
        f"@media (prefers-color-scheme: dark){{{_reglas(COLORES['oscuro'])}}}\n"
        "</style>\n"
        # Hatch: 45-degree lines, tone-on-tone with the fill. Only for the shared engine.
        '<defs><pattern id="trama" patternUnits="userSpaceOnUse" width="6" height="6" '
        'patternTransform="rotate(45)"><rect class="trama-fondo" width="6" height="6"/>'
        '<line class="trama" x1="0" y1="0" x2="0" y2="6" stroke-width="2.5"/></pattern></defs>\n'
    )


def barra_redondeada(x, y, w, h, r, clase):
    """Bar with rounded data end (right) and square baseline end (left)."""
    w = max(w, 1.0)
    r = min(r, w / 2, h / 2)
    d = (f"M{x:.1f},{y:.1f} H{x + w - r:.1f} A{r},{r} 0 0 1 {x + w:.1f},{y + r:.1f} "
         f"V{y + h - r:.1f} A{r},{r} 0 0 1 {x + w - r:.1f},{y + h:.1f} H{x:.1f} Z")
    return f'<path d="{d}" class="{clase}"/>'


# ------------------------------------------------ memory and startup (bars)

def grafica_con_bigote(datos, lang, metrica, unidad, titulo, subtitulo):
    """Horizontal bars: median as the bar, min-max as a thin whisker."""
    t = textos(lang)
    lz = Lienzo()
    lz.lang = lang
    lz.cabecera(titulo, subtitulo)
    lz.leyenda(MARGEN, 80, [("acento", t["visor"]), ("neutro", t["leyenda_otra"])])

    maximo = max(medida(datos, app, caso, metrica)["max"] for caso, apps in CASOS_BIGOTE for app in apps)
    paso, top = paso_eje(maximo * 1.25)
    x0, x1 = 190.0, 550.0
    px = lambda v: x0 + v / top * (x1 - x0)

    y = 104.0
    plot_top = y - 4
    descripcion = []
    for caso, apps in CASOS_BIGOTE:
        lz.texto(MARGEN, y + 12, t["casos"][caso], size=13, weight=600, knock=True)
        y += 24
        for app in apps:
            m = medida(datos, app, caso, metrica)
            yc = y + 15
            clase = "acento" if app == "visor" else "neutro"
            lz.texto(40, yc + 4.5, t[app], size=13, weight=600 if app == "visor" else None)
            lz.add(barra_redondeada(x0, yc - 8, px(m["mediana"]) - x0, 16, 4, clase))
            lo, hi = px(m["min"]), px(m["max"])
            lz.add(f'<line x1="{lo:.1f}" y1="{yc}" x2="{hi:.1f}" y2="{yc}" class="halo"/>')
            lz.add(f'<line x1="{lo:.1f}" y1="{yc}" x2="{hi:.1f}" y2="{yc}" class="bigote"/>')
            for xx in (lo, hi):
                lz.add(f'<line x1="{xx:.1f}" y1="{yc - 6}" x2="{xx:.1f}" y2="{yc + 6}" class="bigote"/>')
            etiqueta_x = max(px(m["mediana"]), hi) + 10
            cabe(etiqueta_x + ancho(f'{numero(m["mediana"], 0, lang)} {unidad} '
                                    f'({numero(m["min"], 0, lang)}–{numero(m["max"], 0, lang)})', 13, 600),
                 ANCHO - MARGEN, "etiqueta de barra")
            lz.add(
                f'<text xml:space="preserve" x="{etiqueta_x:.1f}" y="{yc + 4.5:.1f}" class="t1 knock" font-size="13">'
                f'<tspan font-weight="600">{numero(m["mediana"], 0, lang)} {unidad}\u00a0</tspan>'
                f'<tspan class="t2">({numero(m["min"], 0, lang)}–{numero(m["max"], 0, lang)})</tspan></text>'
            )
            descripcion.append(f"{t['casos'][caso]}, {t[app]}: {numero(m['mediana'], 0, lang)} {unidad}")
            y += 30
        y += 8

    eje_y = y + 2
    # grid goes first, so it sits behind every mark and label
    lz.partes[0:0] = lz.eje_mb(x0, x1, px, top, paso, plot_top, eje_y, unidad)
    lz.pie(eje_y + 60, [t["bigote"]] + pie_lineas(datos, t))
    return lz.render(titulo, "; ".join(descripcion) + ". " + subtitulo)


# ------------------------------------------------------- disk size (stacked)

def grafica_tamano(datos, lang):
    t = textos(lang)
    es = lang == "es"
    v, o, c = tamano(datos, "visor"), tamano(datos, "obsidian"), tamano(datos, "chromium")
    total_v = v["app"] + v["motor"]
    # Engine included: comparing visor's bare binary with whole browsers would flatter it.
    r = [(o["app"] + o["motor"]) / total_v, (c["app"] + c["motor"]) / total_v]
    titulo = titulo_comparacion(lang, r, VERBO_TAMANO)
    subtitulo = ("Espacio en disco (MB), app y motor apilados · Menos es mejor" if es
                 else "Disk space (MB), app and engine stacked · Less is better")

    lz = Lienzo()
    lz.lang = lang
    lz.cabecera(titulo, subtitulo)
    lz.leyenda(MARGEN, 80, [("acento", "visor: app"), ("trama", "visor: motor WebKitGTK (compartido)" if es else "visor: WebKitGTK engine (shared)")])
    lz.leyenda(MARGEN, 100, [("neutro", "Obsidian y Chromium: app" if es else "Obsidian and Chromium: app"),
                             ("neutro-claro", "Obsidian: motor (Electron)" if es else "Obsidian: engine (Electron)")])

    filas = [("visor", v, True), ("obsidian", o, False), ("chromium", c, False)]
    maximo = max(d["app"] + d["motor"] for _, d, _ in filas)
    paso, top = paso_eje(maximo * 1.25)
    x0, x1 = 190.0, 550.0
    px = lambda val: x0 + val / top * (x1 - x0)
    GAP = 2.0  # surface gap between touching segments
    plot_top = 118.0

    y = 124.0
    for app, d, es_visor in filas:
        yc = y + 14
        lz.texto(40, yc + 4.5, t[app], size=13, weight=600 if es_visor else None)
        xa = px(d["app"])
        if d["motor"] > 0:
            # the app segment is square; the engine segment carries the rounded end
            lz.add(f'<rect x="{x0}" y="{yc - 8}" width="{xa - x0 - GAP / 2:.1f}" height="16" '
                   f'class="{"acento" if es_visor else "neutro"}"/>')
            xm = xa + GAP / 2
            fin = px(d["app"] + d["motor"])
            if es_visor:
                lz.add(f'<path d="M{xm:.1f},{yc - 8} H{fin - 4:.1f} A4,4 0 0 1 {fin:.1f},{yc - 4} '
                       f'V{yc + 4} A4,4 0 0 1 {fin - 4:.1f},{yc + 8} H{xm:.1f} Z" fill="url(#trama)"/>')
            else:
                lz.add(barra_redondeada(xm, yc - 8, fin - xm, 16, 4, "neutro-claro"))
        else:
            fin = xa
            lz.add(barra_redondeada(x0, yc - 8, xa - x0, 16, 4, "acento" if es_visor else "neutro"))
        total = d["app"] + d["motor"]
        cabe(fin + 10 + ancho(f"{numero(total, 1, lang)} MB", 13, 600), ANCHO - MARGEN, "total de disco")
        lz.add(f'<text x="{fin + 10:.1f}" y="{yc + 4.5:.1f}" class="t1 knock" font-size="13">'
               f'<tspan font-weight="600">{numero(total, 1, lang)} MB</tspan></text>')
        if d["motor"] > 0:
            if es_visor:
                motor = ("WebKitGTK, biblioteca compartida del sistema" if es
                         else "WebKitGTK, shared system library")
            else:
                motor = "Electron, dentro de su carpeta" if es else "Electron, inside its folder"
            nota = (f"app {numero(d['app'], 1, lang)} MB · {'motor' if es else 'engine'} {numero(d['motor'], 1, lang)} MB ({motor})")
        else:
            nota = (f"app {numero(d['app'], 1, lang)} MB · motor incluido en la app" if es
                    else f"app {numero(d['app'], 1, lang)} MB · engine included in the app")
        lineas = envolver(nota, 12, ANCHO - MARGEN - x0)
        for j, linea in enumerate(lineas):
            lz.texto(x0, yc + 30 + j * 15, linea, clase="t2", size=12, knock=True)
        y += 60 + 15 * (len(lineas) - 1)

    eje_y = y - 2
    lz.partes[0:0] = lz.eje_mb(x0, x1, px, top, paso, plot_top, eje_y, "MB")
    if es:
        aviso = [
            "El motor de visor (WebKitGTK) es una biblioteca compartida del sistema: otras apps también la usan.",
            "Sin él, visor pesa solo su app. Chromium lleva su motor dentro de la app.",
        ]
    else:
        aviso = [
            "visor's engine (WebKitGTK) is a shared system library: other apps use it too.",
            "Without it, visor is only its app. Chromium carries its engine inside the app.",
        ]
    lz.pie(eje_y + 60, aviso + pie_lineas(datos, t))
    descripcion = (
        f"visor: app {v['app']} MB, motor {v['motor']} MB; Obsidian: app {o['app']} MB, motor {o['motor']} MB; "
        f"Chromium: app {c['app']} MB." if es else
        f"visor: app {v['app']} MB, engine {v['motor']} MB; Obsidian: app {o['app']} MB, engine {o['motor']} MB; "
        f"Chromium: app {c['app']} MB."
    )
    return lz.render(titulo, descripcion + " " + subtitulo)


# ----------------------------------------------------------- headlines

def titulo_comparacion(lang, r, verbo):
    """Headline from computed ratios; claims only what the numbers support."""
    es = lang == "es"
    mn, mx = min(r), max(r)
    if mn >= UMBRAL_PRESUMIR:
        return (f"visor {verbo['es']} entre {veces(mn, lang)} y {veces(mx, lang)} {verbo['es_comp']}" if es
                else f"visor {verbo['en']} between {veces(mn, lang)} and {veces(mx, lang)} {verbo['en_comp']}")
    if mx < UMBRAL_PRESUMIR and mn > 1 / UMBRAL_PRESUMIR:
        return (f"visor {verbo['parecido_es']}" if es else f"visor {verbo['parecido_en']}")
    return ("La ventaja de visor depende del caso" if es else "visor's advantage depends on the case")


VERBO_MEMORIA = {
    "es": "usa", "es_comp": "menos memoria que las otras apps", "en": "uses", "en_comp": "less memory than the other apps",
    "parecido_es": "usa una memoria parecida a la de las otras apps",
    "parecido_en": "uses about the same memory as the other apps",
}
VERBO_TAMANO = {
    "es": "ocupa", "es_comp": "menos disco, con su motor incluido",
    "en": "needs", "en_comp": "less disk, engine included",
    "parecido_es": "ocupa, con su motor, un espacio parecido al de las otras apps",
    "parecido_en": "takes up, engine included, about the same disk as the other apps",
}
VERBO_ARRANQUE = {
    "es": "muestra su ventana", "es_comp": "más rápido que las otras apps",
    "en": "shows its window", "en_comp": "faster than the other apps",
    "parecido_es": "arranca a un ritmo parecido al de las otras apps",
    "parecido_en": "starts at about the same speed as the other apps",
}


def grafica_resumen(datos, lang):
    t = textos(lang)
    es = lang == "es"
    r_mem = [ratio(datos, "obsidian", "minima", "memoria_mb"),
             ratio(datos, "obsidian", "ondas", "memoria_mb"),
             ratio(datos, "chromium", "suelto", "memoria_mb")]
    r_arr = [ratio(datos, "obsidian", "minima", "arranque_ms"),
             ratio(datos, "obsidian", "ondas", "arranque_ms"),
             ratio(datos, "chromium", "suelto", "arranque_ms")]
    gana = min(r_mem) >= UMBRAL_PRESUMIR and min(r_arr) >= UMBRAL_PRESUMIR
    if es:
        titulo = ("visor usa menos memoria y arranca antes que Obsidian y Chromium" if gana
                  else "visor frente a Obsidian y Chromium: cifras clave")
        subtitulo = "Cifras clave de la nota sencilla y de la página suelta · Menos es mejor"
    else:
        titulo = ("visor uses less memory and starts faster than Obsidian and Chromium" if gana
                  else "visor against Obsidian and Chromium: key figures")
        subtitulo = "Key figures for the simple note and the loose page · Less is better"

    m_v = medida(datos, "visor", "minima", "memoria_mb")["mediana"]
    m_o = medida(datos, "obsidian", "minima", "memoria_mb")["mediana"]
    a_v = medida(datos, "visor", "minima", "arranque_ms")["mediana"]
    a_o = medida(datos, "obsidian", "minima", "arranque_ms")["mediana"]
    as_v = medida(datos, "visor", "suelto", "arranque_ms")["mediana"]
    as_c = medida(datos, "chromium", "suelto", "arranque_ms")["mediana"]
    cpu_v = medida(datos, "visor", "minima", "cpu_pct")["mediana"]
    cpu_o = medida(datos, "obsidian", "minima", "cpu_pct")["mediana"]

    def tarjeta(r, v, o, unidad, otra, lineas_claim, lineas_num):
        """Big ratio when it is worth claiming; otherwise the plain number."""
        if r >= UMBRAL_PRESUMIR:
            return veces(r, lang), lineas_claim(v, o)
        return f"{numero(v, 0, lang)} {unidad}", lineas_num(o)

    if es:
        tarjetas = [
            tarjeta(r_mem[0], m_v, m_o, "MB", "Obsidian",
                    lambda v, o: ["menos memoria que Obsidian", "(nota sencilla)",
                                  f"{numero(v, 0, lang)} frente a {numero(o, 0, lang)} MB"],
                    lambda o: ["memoria de visor", "(nota sencilla)", f"Obsidian: {numero(o, 0, lang)} MB"]),
            tarjeta(r_arr[0], a_v, a_o, "ms", "Obsidian",
                    lambda v, o: ["más rápido en abrir la ventana que Obsidian", "(nota sencilla)",
                                  f"{numero(v, 0, lang)} frente a {numero(o, 0, lang)} ms"],
                    lambda o: ["arranque de visor", "(nota sencilla)", f"Obsidian: {numero(o, 0, lang)} ms"]),
            tarjeta(r_arr[2], as_v, as_c, "ms", "Chromium",
                    lambda v, o: ["más rápido en abrir la ventana que Chromium", "(página suelta)",
                                  f"{numero(v, 0, lang)} frente a {numero(o, 0, lang)} ms"],
                    lambda o: ["arranque de visor", "(página suelta)", f"Chromium: {numero(o, 0, lang)} ms"]),
            (f"{numero(cpu_v, 2, lang)} %",
             ["CPU en reposo de visor", "(nota sencilla)", f"Obsidian: {numero(cpu_o, 2, lang)} %"]),
        ]
        nota = ["Cifras calculadas de las medianas de cifras.json.",
                "Un cociente menor de 1,2× se muestra como cifra, sin «menos»."]
    else:
        tarjetas = [
            tarjeta(r_mem[0], m_v, m_o, "MB", "Obsidian",
                    lambda v, o: ["less memory than Obsidian", "(simple note)",
                                  f"{numero(v, 0, lang)} vs {numero(o, 0, lang)} MB"],
                    lambda o: ["visor memory", "(simple note)", f"Obsidian: {numero(o, 0, lang)} MB"]),
            tarjeta(r_arr[0], a_v, a_o, "ms", "Obsidian",
                    lambda v, o: ["faster to open its window than Obsidian", "(simple note)",
                                  f"{numero(v, 0, lang)} vs {numero(o, 0, lang)} ms"],
                    lambda o: ["visor startup", "(simple note)", f"Obsidian: {numero(o, 0, lang)} ms"]),
            tarjeta(r_arr[2], as_v, as_c, "ms", "Chromium",
                    lambda v, o: ["faster to open its window than Chromium", "(loose page)",
                                  f"{numero(v, 0, lang)} vs {numero(o, 0, lang)} ms"],
                    lambda o: ["visor startup", "(loose page)", f"Chromium: {numero(o, 0, lang)} ms"]),
            (f"{numero(cpu_v, 2, lang)} %",
             ["visor CPU at rest", "(simple note)", f"Obsidian: {numero(cpu_o, 2, lang)} %"]),
        ]
        nota = ["Figures computed from the medians in cifras.json.",
                "A ratio under 1.2× is shown as a number, without “less”."]

    lz = Lienzo()
    lz.lang = lang
    lz.cabecera(titulo, subtitulo)
    ancho_t = (ANCHO - 2 * MARGEN - 3 * 12) / 4
    interior = ancho_t - 28
    y_t = 76
    envueltas = [[l for p in parrafos for l in envolver(p, 12, interior)] for _, parrafos in tarjetas]
    alto_t = 70 + 18 * max(len(l) for l in envueltas)
    for i, ((valor, _), lineas) in enumerate(zip(tarjetas, envueltas)):
        x = MARGEN + i * (ancho_t + 12)
        lz.add(f'<rect x="{x:.1f}" y="{y_t}" width="{ancho_t:.1f}" height="{alto_t}" rx="8" class="tarjeta"/>')
        lz.texto(x + 14, y_t + 40, valor, size=28, weight=700, hasta=x + ancho_t - 14)
        for j, linea in enumerate(lineas):
            lz.texto(x + 14, y_t + 70 + j * 18, linea, clase="t2", size=12, hasta=x + ancho_t - 14)
    lz.pie(y_t + alto_t + 40, nota + pie_lineas(datos, t))
    return lz.render(titulo, titulo + ". " + subtitulo)


# ------------------------------------------------------------------- main

def generar(datos, lang):
    r_mem = [ratio(datos, "obsidian", "minima", "memoria_mb"),
             ratio(datos, "obsidian", "ondas", "memoria_mb"),
             ratio(datos, "chromium", "suelto", "memoria_mb")]
    r_arr = [ratio(datos, "obsidian", "minima", "arranque_ms"),
             ratio(datos, "obsidian", "ondas", "arranque_ms"),
             ratio(datos, "chromium", "suelto", "arranque_ms")]
    es = lang == "es"
    return {
        "memoria": grafica_con_bigote(
            datos, lang, "memoria_mb", "MB",
            titulo_comparacion(lang, r_mem, VERBO_MEMORIA),
            "Memoria en reposo (PSS, MB) · Menos es mejor" if es
            else "Memory at rest (PSS, MB) · Less is better"),
        "arranque": grafica_con_bigote(
            datos, lang, "arranque_ms", "ms",
            titulo_comparacion(lang, r_arr, VERBO_ARRANQUE),
            "Arranque en frío (ms), desde lanzar la app hasta su ventana · Menos es mejor" if es
            else "Cold start (ms), from launching the app to its window · Less is better"),
        "tamano": grafica_tamano(datos, lang),
        "resumen": grafica_resumen(datos, lang),
    }


def main():
    ap = argparse.ArgumentParser(description="Genera las gráficas SVG del README.")
    ap.add_argument("--datos", type=Path,
                    help="JSON de cifras (por defecto docs/img/cifras.json, o el ejemplo si no existe)")
    ap.add_argument("--salida", type=Path, default=SALIDA_POR_DEFECTO,
                    help="carpeta de salida (por defecto docs/img/graficas)")
    args = ap.parse_args()

    ruta = args.datos
    if ruta is None:
        if DATOS_POR_DEFECTO.exists():
            ruta = DATOS_POR_DEFECTO
        else:
            print(f"graficas: no existe {DATOS_POR_DEFECTO}; uso el ejemplo {EJEMPLO.name}", file=sys.stderr)
            ruta = EJEMPLO
    datos = cargar(ruta)
    if datos.get("ejemplo"):
        print("graficas: AVISO, los datos son de ejemplo (cifras inventadas).", file=sys.stderr)

    args.salida.mkdir(parents=True, exist_ok=True)
    for lang in ("es", "en"):
        for nombre, svg in generar(datos, lang).items():
            destino = args.salida / f"{nombre}.{lang}.svg"
            destino.write_text(svg, encoding="utf-8")
            print(destino)


if __name__ == "__main__":
    main()
