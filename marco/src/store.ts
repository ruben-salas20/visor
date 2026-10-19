// The frame's copy of what Rust sent. Each snapshot replaces its area whole;
// components read their area with `useArea`.

import { useSyncExternalStore } from 'react'
import type { BarState } from './areas/bar/protocol'
import type { FilesState, Tree } from './areas/files/protocol'
import type { FindState } from './areas/find/protocol'
import type { Status } from './areas/settings/protocol'
import type { SidesState } from './areas/sides/protocol'
import { listen, type Colors, type ToFrame } from './protocol'
import { applyColors } from './theme'

export type Store = {
  tree: Tree
  files: FilesState
  sides: SidesState
  bar: BarState
  find: FindState
  status: Status
}

let state: Store = {
  tree: { vault: null, root: null, open: [] },
  files: { shown: true, vault: null, current: null, vaults: [], form: null, notice: null, width: null, show_other: true },
  sides: { sides: [], focused: 0, ratio: 0.5 },
  bar: { sides: [] },
  find: { open: false, side: 0, text: '', match_case: false, whole_word: false, label: '', found: false, edit: 0 },
  status: {
    vault: null,
    kind: 'empty',
    zoom: 1,
    version: '',
    lang: 'es',
    update: null,
    update_busy: false,
    update_restart: false,
    language_pending: false,
    prefs: { theme: null, language: null, search_engine: 'duckduckgo', engines: [], auto_update: true },
    settings_open: false,
    shortcuts_open: false,
  },
}

const listeners = new Set<() => void>()

function receive(m: ToFrame) {
  const { area, ...rest } = m
  if (area === 'theme') {
    applyColors(rest as Colors)
    return
  }
  state = { ...state, [area]: rest }
  for (const f of listeners) f()
}

listen(receive)

function subscribe(f: () => void) {
  listeners.add(f)
  return () => listeners.delete(f)
}

export function useArea<K extends keyof Store>(area: K): Store[K] {
  return useSyncExternalStore(subscribe, () => state[area])
}

/** The current copy, for code outside components. */
export function store() {
  return state
}
