// Area A, the panel. Mirror of src/ui/protocol/files.rs and of
// `SidebarMsg` in src/ui/action.rs.

export type FileKind = 'page' | 'image' | 'pdf' | 'other'

export type File = { name: string; path: string; kind: FileKind }

export type Dir = {
  name: string
  path: string
  /** How many files it lists, counting its subfolders. */
  count: number
  /** What the folder says when it lists nothing. */
  empty: string | null
  dirs: Dir[]
  files: File[]
}

export type Tree = {
  /** The vault's whole path: when it changes, `open` is restored. */
  vault: string | null
  root: Dir | null
  /** The folders the person left open in this vault. */
  open: string[]
}

export type VaultItem = { path: string; name: string; current: boolean }

/** The field that asks for the name of a new note or folder. */
export type Form = { dir: string; folder: boolean; error: string | null; id: number }

export type Notice = { id: number; text: string; error: boolean; elsewhere: string | null }

export type FilesState = {
  /** Hidden for a loose page started alone, and by Ctrl+B. */
  shown: boolean
  vault: string | null
  /** The file on the focused side. */
  current: string | null
  vaults: VaultItem[]
  form: Form | null
  notice: Notice | null
  /** In CSS pixels; none until the person drags the edge. */
  width: number | null
  /** Whether files visor does not open are listed. */
  show_other: boolean
}

/** Paths are vault-relative; Rust checks each one. */
export type FilesMsg =
  | { type: 'pick_vault' }
  | { type: 'switch_vault'; path: string }
  | { type: 'remove_vault'; path: string }
  | { type: 'open'; path: string; tab: boolean; beside: boolean }
  | { type: 'create_note'; dir: string; name: string }
  | { type: 'create_folder'; dir: string; name: string }
  | { type: 'trash'; path: string }
  | { type: 'open_elsewhere'; path: string }
  | { type: 'move'; path: string; dir: string }
  | { type: 'open_folders'; folders: string[] }
  /** Without `dir`, next to the file on the focused side. */
  | { type: 'ask_name'; dir: string | null; folder: boolean }
  | { type: 'cancel_name' }
  /** While it lasts, the pages take the file (area B, src/ui/host.rs). */
  | { type: 'drag_start'; path: string }
  | { type: 'drag_end' }
  /** Over a side's pages: 0 left half, 1 right half, 2 the whole side. */
  | { type: 'drop_hover'; side: number; zone: number | null }
  | { type: 'drop_on'; side: number; half: number | null }
  | { type: 'width'; width: number }
  | { type: 'show_other'; on: boolean }
