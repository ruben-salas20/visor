// Area A, the panel. Mirror of src/ui/protocol/files.rs and of
// `SidebarMsg` in src/ui/action.rs.

export type FileKind = 'page' | 'image' | 'pdf' | 'other'

export type File = { name: string; path: string; kind: FileKind }

export type Dir = { name: string; path: string; dirs: Dir[]; files: File[] }

export type Tree = { root: Dir | null }

export type FilesState = {
  /** Hidden for a loose page started alone, and by Ctrl+B. */
  shown: boolean
  vault: string | null
  /** The file on the focused side. */
  current: string | null
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
  /** While it lasts, the pages take the file (area B, src/ui/host.rs). */
  | { type: 'drag_start'; path: string }
  | { type: 'drag_end' }
