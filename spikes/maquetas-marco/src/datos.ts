// Static content shared by the three mockups. Nothing here talks to Rust.

export type Tipo = 'md' | 'html' | 'pdf'

export type Nodo =
  | { tipo: 'carpeta'; nombre: string; abierta: boolean; hijos: Nodo[] }
  | { tipo: Tipo; nombre: string; ruta: string }

export const BOVEDA = 'Cálculo II'
export const VERSION = 'v0.2.0'
export const ACTUAL = 'Tema 3 · Integrales múltiples/Cambio de variables.md'

export const ARBOL: Nodo[] = [
  { tipo: 'carpeta', nombre: 'Tema 1 · Límites y continuidad', abierta: false, hijos: [] },
  { tipo: 'carpeta', nombre: 'Tema 2 · Derivadas parciales', abierta: false, hijos: [] },
  {
    tipo: 'carpeta',
    nombre: 'Tema 3 · Integrales múltiples',
    abierta: true,
    hijos: [
      {
        tipo: 'carpeta',
        nombre: 'Ejemplos',
        abierta: true,
        hijos: [
          { tipo: 'md', nombre: 'Teorema de Fubini', ruta: 'Tema 3 · Integrales múltiples/Ejemplos/Teorema de Fubini.md' },
          { tipo: 'md', nombre: 'Volumen de un sólido', ruta: 'Tema 3 · Integrales múltiples/Ejemplos/Volumen de un sólido.md' },
        ],
      },
      { tipo: 'md', nombre: 'Cambio de variables', ruta: ACTUAL },
      { tipo: 'html', nombre: 'Coordenadas polares.html', ruta: 'Tema 3 · Integrales múltiples/Coordenadas polares.html' },
      { tipo: 'pdf', nombre: 'Ejercicios resueltos.pdf', ruta: 'Tema 3 · Integrales múltiples/Ejercicios resueltos.pdf' },
      { tipo: 'md', nombre: 'Integrales dobles', ruta: 'Tema 3 · Integrales múltiples/Integrales dobles.md' },
    ],
  },
  { tipo: 'carpeta', nombre: 'Tema 4 · Green y Stokes', abierta: false, hijos: [] },
  { tipo: 'md', nombre: 'Fórmulas', ruta: 'Fórmulas.md' },
  { tipo: 'pdf', nombre: 'Programa de la asignatura.pdf', ruta: 'Programa de la asignatura.pdf' },
]

export type Pestana = { titulo: string; tipo: Tipo; activa?: boolean }

export type Lado = {
  pestanas: Pestana[]
  direccion: string
  atras: boolean
  adelante: boolean
  pagina: 'cambio' | 'dobles'
}

export const LADOS: Lado[] = [
  {
    pestanas: [
      { titulo: 'Cambio de variables', tipo: 'md', activa: true },
      { titulo: 'Coordenadas polares', tipo: 'html' },
      { titulo: 'Resumen del teorema de Fubini para regiones generales', tipo: 'md' },
    ],
    direccion: ACTUAL,
    atras: true,
    adelante: false,
    pagina: 'cambio',
  },
  {
    pestanas: [
      { titulo: 'Integrales dobles', tipo: 'md', activa: true },
      { titulo: 'Programa de la asignatura', tipo: 'pdf' },
    ],
    direccion: 'Tema 3 · Integrales múltiples/Integrales dobles.md',
    atras: false,
    adelante: false,
    pagina: 'dobles',
  },
]

/** The five colors src/ui/theme.rs keeps from a palette. */
export type Paleta = { oscuro: boolean; bg: string; fg: string; accent: string; muted: string; selection: string }

/** ~/.local/state/omarchy/current/theme/colors.toml on 2026-10-19 (Starship Night). */
export const ACTUAL_TEMA: Paleta = {
  oscuro: true,
  bg: '#000000',
  fg: '#ffffff',
  accent: '#ff8a00',
  muted: '#7a7a7a',
  selection: '#3a2a10',
}

/** `CLARO` in src/ui/theme.rs (Omarchy's White). */
export const CLARO: Paleta = {
  oscuro: false,
  bg: '#ffffff',
  fg: '#000000',
  accent: '#6e6e6e',
  muted: '#808080',
  selection: '#c0c0c0',
}

export function aplicarPaleta(p: Paleta) {
  const raiz = document.documentElement
  raiz.classList.toggle('dark', p.oscuro)
  raiz.style.setProperty('--t-bg', p.bg)
  raiz.style.setProperty('--t-fg', p.fg)
  raiz.style.setProperty('--t-accent', p.accent)
  raiz.style.setProperty('--t-muted', p.muted)
  raiz.style.setProperty('--t-selection', p.selection)
}
