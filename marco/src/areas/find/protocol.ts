// Area D, the find bar. Mirror of src/ui/protocol/find.rs: change both at once.

/** The bar's whole state. `open` false hides it. */
export type FindState = {
  open: boolean
  /** The side whose page the bar lies over. */
  side: number
  text: string
  match_case: boolean
  whole_word: boolean
  /** The counter, already in the visor's language: "3 de 12", "Sin resultados". */
  label: string
  /** Whether the search has matches, which turns the step buttons on. */
  found: boolean
  /** Goes up each time the bar takes the keyboard; the frame then focuses its field. */
  edit: number
}

export type FindMsg =
  | { type: 'search'; text: string; match_case: boolean; whole_word: boolean }
  | { type: 'step'; forward: boolean }
  | { type: 'close' }
