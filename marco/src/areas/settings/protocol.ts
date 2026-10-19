// Area E: settings, shortcuts and the status bar. Mirror of src/ui/protocol/settings.rs.

import type { TabKind } from '../sides/protocol'

export type Status = {
  vault: string | null
  /** What the focused side shows. */
  kind: TabKind
  /** 1 for 100 %. */
  zoom: number
  version: string
  /** The last result of an update check, if any. */
  update: string | null
}

/** Nothing yet: settings and shortcuts come in phase 1. */
export type SettingsMsg = never
