// What the frame and Rust say to each other. Mirror of src/ui/protocol.rs:
// change both at once. Each area's own types are in src/areas/<area>/protocol.ts.
// The frame only draws and asks; Rust decides and answers with snapshots.

import type { BarMsg, BarState } from './areas/bar/protocol'
import type { FilesMsg, FilesState, Tree } from './areas/files/protocol'
import type { FindMsg } from './areas/find/protocol'
import type { SettingsMsg, Status } from './areas/settings/protocol'
import type { SidesMsg, SidesState } from './areas/sides/protocol'

/** The five colors of visor's theme (src/ui/theme.rs). */
export type Colors = { dark: boolean; bg: string; fg: string; accent: string; muted: string; selection: string }

/** Where one side's pages go, in CSS pixels of the frame. */
export type Hole = { side: number; x: number; y: number; w: number; h: number }

/** What Rust sends: one area's whole state, which replaces the frame's copy. */
export type ToFrame =
  | ({ area: 'theme' } & Colors)
  | ({ area: 'tree' } & Tree)
  | ({ area: 'files' } & FilesState)
  | ({ area: 'sides' } & SidesState)
  | ({ area: 'bar' } & BarState)
  | ({ area: 'status' } & Status)

/** What the frame asks for. */
export type FrameMsg =
  | { area: 'frame'; type: 'ready' }
  | { area: 'frame'; type: 'holes'; holes: Hole[] }
  | ({ area: 'files' } & FilesMsg)
  | ({ area: 'sides' } & SidesMsg)
  | ({ area: 'bar' } & BarMsg)
  | ({ area: 'find' } & FindMsg)
  | ({ area: 'settings' } & SettingsMsg)

type Bridge = { post(m: FrameMsg): void; listen(f: (m: ToFrame) => void): void }

declare global {
  interface Window {
    /** Set by src/ui/frame-bridge.js, which holds the key. Absent outside visor. */
    __visorFrame?: Bridge
  }
}

export function post(m: FrameMsg) {
  window.__visorFrame?.post(m)
}

export function listen(f: (m: ToFrame) => void) {
  window.__visorFrame?.listen(f)
}
