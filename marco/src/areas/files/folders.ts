// Which folders of the tree are open. The ones the person opens are saved
// (Rust keeps them per vault); the ones opened only to show the file on
// screen, or under a drag, are not.
import { useState } from 'react'
import { post } from '@/protocol'
import type { Tree } from './protocol'

/** The folders that hold `path`: «a/b/c.md» gives «a» and «a/b». */
export function parents(path: string) {
  const parts = path.split('/')
  return parts.slice(0, -1).map((_, i) => parts.slice(0, i + 1).join('/'))
}

export type Folders = {
  isOpen(path: string): boolean
  toggle(path: string): void
  /** Shows a folder without saving it as open. */
  reveal(path: string): void
  /** Opens a folder and the ones above it, and saves them. */
  keep(path: string): void
}

export function useFolders(tree: Tree, current: string | null): Folders {
  const [saved, setSaved] = useState<Set<string>>(new Set())
  const [shown, setShown] = useState<Set<string>>(new Set())
  const [vault, setVault] = useState<string | null | undefined>(undefined)
  const [seen, setSeen] = useState<string | null>(null)
  // Another vault: its own saved folders.
  if (vault !== tree.vault) {
    setVault(tree.vault)
    setSaved(new Set(tree.open))
    setShown(new Set())
  }
  // Only a new file on screen opens its folders, so one the person closed stays closed.
  if (seen !== current) {
    setSeen(current)
    if (current) setShown((s) => new Set([...s, ...parents(current)]))
  }
  const save = (next: Set<string>) => {
    setSaved(next)
    post({ area: 'files', type: 'open_folders', folders: [...next] })
  }
  const isOpen = (path: string) => saved.has(path) || shown.has(path)
  return {
    isOpen,
    toggle(path) {
      const next = new Set(saved)
      if (isOpen(path)) {
        next.delete(path)
        setShown((s) => new Set([...s].filter((p) => p !== path)))
      } else {
        next.add(path)
      }
      save(next)
    },
    reveal(path) {
      setShown((s) => new Set([...s, path]))
    },
    keep(path) {
      save(new Set([...saved, ...parents(path + '/x')]))
    },
  }
}
