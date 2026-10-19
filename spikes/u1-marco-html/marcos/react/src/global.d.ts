// The IPC bridge that wry injects, and the hooks Rust calls or reads.
export {};

declare global {
  interface Window {
    ipc: { postMessage(cuerpo: string): void };
    marco: { pintar(est: Estado): void };
    __ev: string[];
  }
}

export type Pestana = { id: number; ruta: string; activa: boolean };
export type Panel = { pestanas: Pestana[] };
export type Estado = { arbol: string[]; paneles: Panel[] };
