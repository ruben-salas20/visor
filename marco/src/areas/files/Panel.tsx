// Area A, the panel: the vault's name and its tree, read only for now.
// Still to come (phase 1): switching vaults, new note and folder, menus,
// moving, the trash, dragging to a side and keeping open folders.
import { ChevronDown, ChevronRight, FilePlus2, Folder, FolderOpen, FolderPlus } from 'lucide-react'
import { useEffect, useState } from 'react'
import { KindIcon, kindOfPath } from '@/components/kind-icon'
import { Button } from '@/components/ui/button'
import { ScrollArea } from '@/components/ui/scroll-area'
import { cn } from '@/lib/utils'
import { post } from '@/protocol'
import { useArea } from '@/store'
import type { Dir, File } from './protocol'

const fade = 'min-w-0 flex-1 overflow-hidden whitespace-nowrap [mask-image:linear-gradient(to_right,black_calc(100%-28px),transparent)]'

/** The folders that hold `path`: «a/b/c.md» gives «a» and «a/b». */
function parents(path: string) {
  const parts = path.split('/')
  return parts.slice(0, -1).map((_, i) => parts.slice(0, i + 1).join('/'))
}

type Open = { open: Set<string>; toggle: (path: string) => void; current: string | null }

function FolderRow({ dir, level, tree }: { dir: Dir; level: number; tree: Open }) {
  const open = tree.open.has(dir.path)
  const Icon = open ? FolderOpen : Folder
  return (
    <>
      <div
        style={{ paddingLeft: 8 + level * 14 }}
        onClick={() => tree.toggle(dir.path)}
        className="flex h-6 items-center gap-1.5 rounded-md pr-2 text-xs font-semibold text-muted-foreground hover:bg-muted hover:text-foreground"
      >
        <ChevronRight className={cn('size-3.5 shrink-0 transition-transform', open && 'rotate-90')} />
        <Icon className="size-3.5 shrink-0" />
        <span className={fade}>{dir.name}</span>
        <span className="ml-2 text-[11px] font-normal tabular-nums opacity-70">{dir.dirs.length + dir.files.length}</span>
      </div>
      {open && <Rows dir={dir} level={level + 1} tree={tree} />}
    </>
  )
}

function FileRow({ file, level, current }: { file: File; level: number; current: boolean }) {
  const kind = file.kind === 'pdf' ? 'pdf' : file.kind === 'image' ? 'image' : kindOfPath(file.path)
  // Files visor cannot show are listed, but not opened: phase 1 says why.
  const open = () => file.kind !== 'other' && post({ area: 'files', type: 'open', path: file.path, tab: false, beside: false })
  return (
    <div
      style={{ paddingLeft: 8 + level * 14 + 20 }}
      onClick={open}
      title={file.path}
      className={cn(
        'flex h-6 items-center gap-2 rounded-md pr-2 text-[13px] text-foreground/90',
        current ? 'bg-selection font-medium text-selection-foreground' : 'hover:bg-accent',
      )}
    >
      <KindIcon kind={kind} className={current ? 'text-primary' : 'text-muted-foreground'} />
      <span className={fade}>{file.name}</span>
    </div>
  )
}

function Rows({ dir, level, tree }: { dir: Dir; level: number; tree: Open }) {
  return (
    <>
      {dir.dirs.map((d) => (
        <FolderRow key={d.path} dir={d} level={level} tree={tree} />
      ))}
      {dir.files.map((f) => (
        <FileRow key={f.path} file={f} level={level} current={f.path === tree.current} />
      ))}
    </>
  )
}

export function Panel() {
  const files = useArea('files')
  const { root } = useArea('tree')
  const [open, setOpen] = useState<Set<string>>(new Set())
  // The folders of the file on screen open when it changes.
  useEffect(() => {
    if (files.current) setOpen((o) => new Set([...o, ...parents(files.current!)]))
  }, [files.current])
  const toggle = (path: string) =>
    setOpen((o) => {
      const next = new Set(o)
      if (!next.delete(path)) next.add(path)
      return next
    })
  if (!files.shown) return null
  return (
    <aside className="flex h-full w-[264px] shrink-0 flex-col border-r bg-sidebar">
      <div className="flex h-[30px] shrink-0 items-center gap-0.5 border-b px-1.5">
        <Button variant="ghost" size="sm" className="h-6 min-w-0 flex-1 justify-start px-2 text-[13px] font-semibold" title="Cambiar de bóveda">
          <span className="truncate">{files.vault ?? 'Sin bóveda'}</span>
          <ChevronDown className="size-3.5 text-muted-foreground" />
        </Button>
        <Button variant="ghost" size="icon-sm" className="text-muted-foreground" title="Nueva nota (Ctrl+N)">
          <FilePlus2 />
        </Button>
        <Button variant="ghost" size="icon-sm" className="text-muted-foreground" title="Nueva carpeta">
          <FolderPlus />
        </Button>
      </div>
      <div className="px-4 pt-2 pb-1 text-[11px] font-semibold tracking-wide text-muted-foreground uppercase">Archivos</div>
      <ScrollArea className="min-h-0 flex-1">
        <div className="flex flex-col px-2 pb-3">{root && <Rows dir={root} level={0} tree={{ open, toggle, current: files.current }} />}</div>
      </ScrollArea>
    </aside>
  )
}
