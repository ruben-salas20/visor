// The vault's tree: open with a click, a new tab with Ctrl+click or the middle
// button, the other side with Ctrl+Alt+click; drag to move; right click for
// the menu; arrows, Enter and Delete with the keyboard.
import { ChevronRight, Folder, FolderOpen } from 'lucide-react'
import { useEffect, useRef, useState, type KeyboardEvent, type MouseEvent } from 'react'
import { KindIcon, kindOfPath } from '@/components/kind-icon'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuShortcut,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { ScrollArea } from '@/components/ui/scroll-area'
import { cn } from '@/lib/utils'
import { post } from '@/protocol'
import { listenDrag, pressRow, type Drag } from './drag'
import type { Folders } from './folders'
import type { Dir, File } from './protocol'

const fade = 'min-w-0 flex-1 overflow-hidden whitespace-nowrap [mask-image:linear-gradient(to_right,black_calc(100%-28px),transparent)]'
const row = 'flex h-6 items-center rounded-md pr-2 outline-none focus-visible:ring-1 focus-visible:ring-ring'

/** The folder that holds `path`; empty for the vault root. */
function parentOf(path: string) {
  const i = path.lastIndexOf('/')
  return i < 0 ? '' : path.slice(0, i)
}

function open(path: string, e: { ctrlKey: boolean; metaKey: boolean; altKey: boolean } | null) {
  const ctrl = !!e && (e.ctrlKey || e.metaKey)
  const beside = ctrl && !!e?.altKey
  post({ area: 'files', type: 'open', path, tab: ctrl && !beside, beside })
}

/** What the menu was opened on: a folder, a file, or the empty space (the root). */
type Target = { path: string; kind: 'dir' | 'file' | 'other' | 'root' }

type Ctx = {
  folders: Folders
  current: string | null
  /** The folder a drop would go into; '' for the root. */
  drop: string | null
}

function FolderRow({ dir, level, ctx }: { dir: Dir; level: number; ctx: Ctx }) {
  const isOpen = ctx.folders.isOpen(dir.path)
  const Icon = isOpen ? FolderOpen : Folder
  const target = ctx.drop === dir.path
  return (
    <>
      <div
        data-row=""
        data-path={dir.path}
        data-kind="dir"
        tabIndex={-1}
        style={{ paddingLeft: 8 + level * 14 }}
        onClick={() => ctx.folders.toggle(dir.path)}
        onPointerDown={(e) => pressRow(e, dir.path, true)}
        className={cn(
          row,
          'gap-1.5 text-xs font-semibold',
          target ? 'bg-selection text-selection-foreground' : 'text-muted-foreground hover:bg-muted hover:text-foreground',
        )}
      >
        <ChevronRight className={cn('size-3.5 shrink-0 transition-transform', isOpen && 'rotate-90')} />
        <Icon className="size-3.5 shrink-0" />
        <span className={fade}>{dir.name}</span>
        <span className="ml-2 text-[11px] font-normal tabular-nums opacity-70">{dir.count}</span>
      </div>
      {isOpen && <Rows dir={dir} level={level + 1} ctx={ctx} />}
    </>
  )
}

function FileRow({ file, level, ctx }: { file: File; level: number; ctx: Ctx }) {
  const kind = file.kind === 'pdf' ? 'pdf' : file.kind === 'image' ? 'image' : kindOfPath(file.path)
  const other = file.kind === 'other'
  const current = file.path === ctx.current
  return (
    <div
      data-row=""
      data-path={file.path}
      data-kind={other ? 'other' : 'file'}
      tabIndex={-1}
      style={{ paddingLeft: 8 + level * 14 + 20 }}
      onClick={(e) => open(file.path, e)}
      // The middle button: a new tab, as for links.
      onMouseDown={(e) => e.button === 1 && e.preventDefault()}
      onAuxClick={(e) => e.button === 1 && post({ area: 'files', type: 'open', path: file.path, tab: true, beside: false })}
      // Files visor does not open stay where they are, as in the GTK panel.
      onPointerDown={(e) => !other && pressRow(e, file.path, false)}
      title={file.path}
      className={cn(
        row,
        'gap-2 text-[13px]',
        current ? 'bg-selection font-medium text-selection-foreground' : 'text-foreground/90 hover:bg-accent',
        other && 'italic opacity-60',
      )}
    >
      <KindIcon kind={kind} className={current ? 'text-primary' : 'text-muted-foreground'} />
      <span className={fade}>{file.name}</span>
    </div>
  )
}

function Rows({ dir, level, ctx }: { dir: Dir; level: number; ctx: Ctx }) {
  return (
    <>
      {dir.empty && (
        <div data-placeholder="" data-path={dir.path} style={{ paddingLeft: 8 + level * 14 + 20 }} className="flex h-6 items-center text-xs text-muted-foreground italic">
          {dir.empty}
        </div>
      )}
      {dir.dirs.map((d) => (
        <FolderRow key={d.path} dir={d} level={level} ctx={ctx} />
      ))}
      {dir.files.map((f) => (
        <FileRow key={f.path} file={f} level={level} ctx={ctx} />
      ))}
    </>
  )
}

function Menu({ target }: { target: Target | null }) {
  if (!target) return null
  const { path, kind } = target
  const files = (m: { type: 'trash' | 'open_elsewhere'; path: string }) => post({ area: 'files', ...m })
  const ask = (folder: boolean) => post({ area: 'files', type: 'ask_name', dir: path, folder })
  if (kind === 'root' || kind === 'dir') {
    return (
      <>
        <ContextMenuItem onSelect={() => ask(false)}>{kind === 'root' ? 'Nueva nota' : 'Nueva nota aquí'}</ContextMenuItem>
        <ContextMenuItem onSelect={() => ask(true)}>{kind === 'root' ? 'Nueva carpeta' : 'Nueva carpeta aquí'}</ContextMenuItem>
        {kind === 'dir' && (
          <>
            <ContextMenuSeparator />
            <ContextMenuItem variant="destructive" onSelect={() => files({ type: 'trash', path })}>
              Mover a la papelera<ContextMenuShortcut>Supr</ContextMenuShortcut>
            </ContextMenuItem>
          </>
        )}
      </>
    )
  }
  return (
    <>
      {kind === 'file' && (
        <>
          <ContextMenuItem onSelect={() => open(path, null)}>Abrir</ContextMenuItem>
          <ContextMenuItem onSelect={() => post({ area: 'files', type: 'open', path, tab: true, beside: false })}>
            Abrir en una pestaña nueva<ContextMenuShortcut>Ctrl+clic</ContextMenuShortcut>
          </ContextMenuItem>
          <ContextMenuItem onSelect={() => post({ area: 'files', type: 'open', path, tab: false, beside: true })}>
            Abrir en el otro lado<ContextMenuShortcut>Ctrl+Alt+clic</ContextMenuShortcut>
          </ContextMenuItem>
        </>
      )}
      <ContextMenuItem onSelect={() => files({ type: 'open_elsewhere', path })}>Abrir con otra aplicación</ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuItem variant="destructive" onSelect={() => files({ type: 'trash', path })}>
        Mover a la papelera<ContextMenuShortcut>Supr</ContextMenuShortcut>
      </ContextMenuItem>
    </>
  )
}

/** The row an event happened on, as the menu and the keyboard read it. */
function targetOf(el: EventTarget | null): Target | null {
  const node = el instanceof Element ? el.closest('[data-row], [data-placeholder]') : null
  if (!node) return { path: '', kind: 'root' }
  if (node.hasAttribute('data-placeholder')) return null
  return { path: node.getAttribute('data-path') ?? '', kind: (node.getAttribute('data-kind') ?? 'file') as Target['kind'] }
}

export function Tree({ root, folders, current }: { root: Dir; folders: Folders; current: string | null }) {
  const box = useRef<HTMLDivElement>(null)
  const [drop, setDrop] = useState<string | null>(null)
  const [menu, setMenu] = useState<Target | null>(null)
  const timer = useRef<number | undefined>(undefined)

  // The file on screen scrolls into view when it changes.
  useEffect(() => {
    if (!current) return
    const id = requestAnimationFrame(() => box.current?.querySelector(`[data-path="${CSS.escape(current)}"]`)?.scrollIntoView({ block: 'nearest' }))
    return () => cancelAnimationFrame(id)
  }, [current])

  const scroller = useRef<HTMLDivElement>(null)
  const [ghost, setGhost] = useState<Drag | null>(null)
  const dropRef = useRef<string | null>(null)
  const folderRef = useRef(folders)
  folderRef.current = folders

  // The folder under a point of the drag; null outside the tree.
  const folderAt = (x: number, y: number) => {
    const el = document.elementFromPoint(x, y)
    if (!el || !scroller.current?.contains(el)) return null
    const node = el.closest('[data-row], [data-placeholder]')
    if (!node) return ''
    const path = node.getAttribute('data-path') ?? ''
    return node.getAttribute('data-kind') === 'dir' || node.hasAttribute('data-placeholder') ? path : parentOf(path)
  }
  const target = (dir: string | null) => {
    if (dir === dropRef.current) return
    dropRef.current = dir
    setDrop(dir)
    window.clearTimeout(timer.current)
    // A closed folder opens after a moment under the pointer.
    if (dir && !folderRef.current.isOpen(dir)) timer.current = window.setTimeout(() => folderRef.current.reveal(dir), 700)
  }
  useEffect(
    () =>
      listenDrag((d, dropped) => {
        if (!d) {
          target(null)
          setGhost(null)
          return
        }
        const dir = folderAt(d.x, d.y)
        if (!dropped) {
          target(dir)
          setGhost(d)
          return
        }
        const into = dir === d.path || dir?.startsWith(d.path + '/')
        if (dir !== null && !into && dir !== parentOf(d.path)) post({ area: 'files', type: 'move', path: d.path, dir })
      }),
    // `target` and `folderAt` only read refs.
    [],
  )
  const ctx: Ctx = { folders, current, drop }

  const keys = (e: KeyboardEvent) => {
    const rows = [...(box.current?.querySelectorAll<HTMLElement>('[data-row]') ?? [])]
    const el = (document.activeElement as HTMLElement | null)?.closest<HTMLElement>('[data-row]')
    const at = el ? rows.indexOf(el) : -1
    const t = el && targetOf(el)
    const step = (n: number) => rows[Math.max(0, Math.min(rows.length - 1, at + n))]?.focus()
    switch (e.key) {
      case 'ArrowDown':
        step(1)
        break
      case 'ArrowUp':
        step(at < 0 ? 1 : -1)
        break
      case 'ArrowRight':
      case 'ArrowLeft':
        if (t?.kind === 'dir' && folders.isOpen(t.path) === (e.key === 'ArrowLeft')) folders.toggle(t.path)
        break
      case 'Enter':
        if (t?.kind === 'dir') folders.toggle(t.path)
        else if (t) open(t.path, e)
        break
      case 'Delete':
        if (t && t.kind !== 'root') post({ area: 'files', type: 'trash', path: t.path })
        break
      default:
        return
    }
    e.preventDefault()
  }

  return (
    <ContextMenu>
      <ContextMenuTrigger
        asChild
        onContextMenu={(e: MouseEvent) => {
          const t = targetOf(e.target)
          if (!t) return e.preventDefault()
          setMenu(t)
        }}
      >
        <ScrollArea
          ref={scroller}
          className={cn('min-h-0 flex-1 [&_[data-slot=scroll-area-viewport]>div]:!block', drop === '' && 'bg-selection/40')}
        >
          <div
            ref={box}
            tabIndex={0}
            onFocus={(e) => e.target === box.current && box.current.querySelector<HTMLElement>('[data-row]')?.focus()}
            onKeyDown={keys}
            className="flex flex-col px-2 pb-3 outline-none select-none"
          >
            <Rows dir={root} level={0} ctx={ctx} />
          </div>
        </ScrollArea>
      </ContextMenuTrigger>
      {ghost && (
        <div
          style={{ left: ghost.x + 12, top: ghost.y + 8 }}
          className="pointer-events-none fixed z-50 max-w-60 truncate rounded-md border bg-popover px-2 py-1 text-xs text-popover-foreground shadow-md"
        >
          {ghost.path.split('/').pop()}
        </div>
      )}
      <ContextMenuContent className="min-w-52">
        <Menu target={menu} />
      </ContextMenuContent>
    </ContextMenu>
  )
}
