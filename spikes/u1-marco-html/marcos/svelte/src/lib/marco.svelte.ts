import { tick } from 'svelte';

// Same protocol as web/marco.html and mensaje_marco in src/main.rs. Change both sides together.

export type Pestana = { id: number; ruta: string; activa: boolean };
export type Panel = { pestanas: Pestana[] };
export type Estado = { arbol: string[]; paneles: Panel[] };

declare global {
	interface Window {
		ipc: { postMessage(cuerpo: string): void };
		marco: { pintar(est: Estado): void };
		__ev: string[];
	}
}

export const estado: Estado = $state({ arbol: [], paneles: [{ pestanas: [] }, { pestanas: [] }] });
export const ui = $state({ foco: 0 });

// Last file opened from the tree: «Nueva pestaña» opens it again.
let seleccion: string | null = null;
// File pressed in the tree. On mouseup outside the tree, Rust decides the panel by coordinates.
let pendiente: string | null = null;

function enviar(mensaje: Record<string, unknown>) {
	window.ipc.postMessage(JSON.stringify(mensaje));
}

// Rust places the page views over these boxes, so they must stay empty.
function enviarRectas() {
	const rectas = [0, 1].map((lado) => {
		const b = document.querySelector(`.contenido[data-lado="${lado}"]`)!.getBoundingClientRect();
		return { x: b.left, y: b.top, w: b.width, h: b.height };
	});
	enviar({ accion: 'recolocar', rectas });
}

export function abrirDesdeArbol(ruta: string) {
	seleccion = ruta;
	enviar({ accion: 'abrir', lado: ui.foco, ruta });
}

export function marcarArrastre(ruta: string) {
	pendiente = ruta;
}

export function nuevaPestana(lado: number) {
	enviar({ accion: 'nueva', lado, ruta: seleccion ?? estado.arbol[0] });
}

export function activar(lado: number, id: number) {
	enviar({ accion: 'activar', lado, id });
}

export function cerrar(lado: number, id: number) {
	enviar({ accion: 'cerrar', lado, id });
}

function pintar(est: Estado) {
	if (estado.arbol.length === 0) estado.arbol = est.arbol;
	estado.paneles = est.paneles;
	// The boxes can only be measured after Svelte has updated the DOM.
	tick().then(enviarRectas);
}

export function iniciar() {
	window.marco = { pintar };
	window.__ev = [];
	// Log of mouse and drag events; the test harness reads window.__ev.
	for (const t of ['mousedown', 'mouseup', 'dragstart', 'dragend'] as const) {
		document.addEventListener(t, (e) => window.__ev.push(`${t}@${Math.round(e.clientX)},${Math.round(e.clientY)}`), true);
	}
	document.addEventListener('mouseup', (e) => {
		if (!pendiente) return;
		const arbol = document.getElementById('arbol')!.getBoundingClientRect();
		if (e.clientX > arbol.right) enviar({ accion: 'soltar', x: e.clientX, y: e.clientY, ruta: pendiente });
		pendiente = null;
	});
	// Clicking a panel makes it the one that receives Ctrl+W and tree clicks.
	document.addEventListener('mousedown', (e) => {
		const p = (e.target as Element).closest<HTMLElement>('.panel');
		if (p) ui.foco = Number(p.dataset.lado);
	});
	document.addEventListener('keydown', (e) => {
		if (e.ctrlKey && e.key.toLowerCase() === 'w') {
			e.preventDefault();
			enviar({ accion: 'cerrar_activa', lado: ui.foco });
		}
	});
	new ResizeObserver(enviarRectas).observe(document.getElementById('paneles')!);
	enviar({ accion: 'listo' });
}
