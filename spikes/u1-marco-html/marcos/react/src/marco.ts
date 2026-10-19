// Frame state and the message protocol with Rust. Rust decides everything;
// this file only draws what Rust sends and reports clicks, drops and resizes.
import { flushSync } from 'react-dom'
import type { Estado, Panel } from './global'

type Instantanea = { arbol: string[]; paneles: Panel[]; foco: number };

// Width of the tree in CSS pixels. Rust does not know it; the drop test uses it.
const ANCHO_ARBOL = 260;

let instantanea: Instantanea = {
  arbol: [],
  paneles: [{ pestanas: [] }, { pestanas: [] }],
  foco: 0,
};
// The last file clicked in the tree: `nueva` opens it in a new tab.
let seleccion: string | null = null;
// The file under the pointer at mousedown; it is dropped on a panel if the pointer
// is released outside the tree.
let pendiente: string | null = null;
const oyentes = new Set<(s: Instantanea) => void>();

function emitir() {
  for (const f of oyentes) f(instantanea);
}

export function leer(): Instantanea {
  return instantanea;
}

export function suscribir(f: (s: Instantanea) => void): () => void {
  oyentes.add(f);
  return () => oyentes.delete(f);
}

function enviar(m: Record<string, unknown>) {
  window.ipc.postMessage(JSON.stringify(m));
}

// Rust places each page webview over one `.contenido` rectangle.
function enviarRectas() {
  const rectas = [0, 1].map((i) => {
    const b = document.querySelector<HTMLElement>(`.contenido[data-lado="${i}"]`)!.getBoundingClientRect();
    return { x: b.left, y: b.top, w: b.width, h: b.height };
  });
  enviar({ accion: 'recolocar', rectas });
}

export const acciones = {
  abrirDesdeArbol(ruta: string) {
    seleccion = ruta;
    enviar({ accion: 'abrir', lado: instantanea.foco, ruta });
  },
  nuevaPestana(lado: number) {
    enviar({ accion: 'nueva', lado, ruta: seleccion || instantanea.arbol[0] });
  },
  activar(lado: number, id: number) {
    enviar({ accion: 'activar', lado, id });
  },
  cerrar(lado: number, id: number) {
    enviar({ accion: 'cerrar', lado, id });
  },
  enviarListo() {
    enviar({ accion: 'listo' });
  },
  enviarRectas,
  pendienteAl(ruta: string | null) {
    pendiente = ruta;
  },
};

function setFoco(lado: number) {
  if (lado === instantanea.foco) return;
  instantanea = { ...instantanea, foco: lado };
  emitir();
}

// Protocol-level listeners, kept native as in the reference frame (marco.html).
// Event log used by the test harness (prueba f).
window.__ev = [];
for (const t of ['mousedown', 'mouseup', 'dragstart', 'dragend']) {
  document.addEventListener(
    t,
    (e) => {
      const m = e as MouseEvent;
      window.__ev.push(`${t}@${Math.round(m.clientX)},${Math.round(m.clientY)}`);
    },
    true,
  );
}
document.addEventListener('mouseup', (e) => {
  if (!pendiente) return;
  if (e.clientX > ANCHO_ARBOL) enviar({ accion: 'soltar', x: e.clientX, y: e.clientY, ruta: pendiente });
  pendiente = null;
});
document.addEventListener('mousedown', (e) => {
  const p = (e.target as Element).closest<HTMLElement>('.panel');
  if (p) setFoco(Number(p.dataset.lado));
});
document.addEventListener('keydown', (e) => {
  if (e.ctrlKey && e.key.toLowerCase() === 'w') {
    e.preventDefault();
    enviar({ accion: 'cerrar_activa', lado: instantanea.foco });
  }
});

// Rust calls pintar() after each change. The DOM is updated synchronously so that
// the rectangles sent right after are the ones on screen.
window.marco = {
  pintar(est: Estado) {
    const arbol = instantanea.arbol.length ? instantanea.arbol : est.arbol;
    instantanea = { ...instantanea, arbol, paneles: est.paneles };
    flushSync(emitir);
    enviarRectas();
  },
};
