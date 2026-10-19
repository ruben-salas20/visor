// Area C, the address row of each side. Mirror of src/ui/protocol/bar.rs.

export type Address = {
  side: number
  /** Vault path, absolute path of a loose page, web address, or empty. */
  target: string
  back: boolean
  forward: boolean
  /** Only vault pages have annotations. */
  annotations: boolean
  /** Goes up each time Rust asks for the field (Ctrl+L). */
  edit: number
  /** Goes up each time a typed address led nowhere. */
  refused: number
}

export type BarState = { sides: Address[] }

export type BarMsg =
  | { type: 'back'; side: number }
  | { type: 'forward'; side: number }
  | { type: 'reload'; side: number }
  | { type: 'pick_element'; side: number }
  | { type: 'toggle_annotations'; side: number }
  | { type: 'go'; side: number; text: string }
