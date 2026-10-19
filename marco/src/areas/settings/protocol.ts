// Area E: settings, shortcuts and the status bar. Mirror of src/ui/protocol/settings.rs.

import type { TabKind } from '../sides/protocol'

export type Choice = { id: string; label: string }

/** The choices of the settings panel, as they are saved. `null` is the system's. */
export type Prefs = {
  theme: string | null
  language: string | null
  search_engine: string
  engines: Choice[]
  auto_update: boolean
}

export type Panel = 'settings' | 'shortcuts'

export type Status = {
  vault: string | null
  /** What the focused side shows. */
  kind: TabKind
  /** 1 for 100 %. */
  zoom: number
  version: string
  /** The interface language in use: the frame's texts follow it. */
  lang: 'es' | 'en'
  /** The last result of an update check or a restart, if any. */
  update: string | null
  update_busy: boolean
  update_restart: boolean
  language_pending: boolean
  prefs: Prefs
  settings_open: boolean
  shortcuts_open: boolean
}

/** What the frame asks for. Rust decides and answers with the status. */
export type SettingsMsg =
  | { type: 'open'; panel: Panel }
  | { type: 'closed'; panel: Panel }
  | { type: 'set_theme'; choice: string | null }
  | { type: 'set_language'; choice: string | null }
  | { type: 'zoom'; step: number }
  | { type: 'set_search_engine'; id: string }
  | { type: 'set_auto_update'; on: boolean }
  | { type: 'check_updates' }
  | { type: 'restart' }
