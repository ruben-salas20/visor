// Area B, the sides and their tab rows. Mirror of src/ui/protocol/sides.rs.

export type TabKind = 'empty' | 'note' | 'drawing' | 'html' | 'pdf' | 'image' | 'web' | 'other'

export type Tab = { id: number; label: string; target: string; kind: TabKind }

export type Side = { id: number; tabs: Tab[]; active: number }

/** `ratio`: the left side's share of the width, when split. */
export type SidesState = { sides: Side[]; focused: number; ratio: number }

export type SidesMsg =
  | { type: 'select'; tab: number }
  | { type: 'close'; tab: number }
  | { type: 'new'; side: number }
  | { type: 'focus'; side: number }
  | { type: 'split'; ratio: number }
