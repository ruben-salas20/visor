// Area A, the panel: the vault menu, new notes and folders, the tree, the
// notices, and the edge that changes its width.
import { ChevronDown, Ellipsis, FilePlus2, FolderPlus, X } from 'lucide-react'
import { useEffect, useRef, useState, type PointerEvent } from 'react'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { cn } from '@/lib/utils'
import { post } from '@/protocol'
import { useArea } from '@/store'
import { useFolders, type Folders } from './folders'
import type { FilesState, Form, Notice } from './protocol'
import { Tree } from './Tree'

/** The width Rust keeps it in; see `clamp_width` in src/ui/protocol/files.rs. */
const WIDTH = { min: 180, max: 560, start: 264 }

function Vaults({ files }: { files: FilesState }) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="sm" className="h-6 min-w-0 flex-1 justify-start px-2 text-[13px] font-semibold" title="Cambiar de bóveda">
          <span className="truncate">{files.vault ?? 'Sin bóveda'}</span>
          <ChevronDown className="size-3.5 text-muted-foreground" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-60">
        {files.vaults.length > 0 && <DropdownMenuLabel>Bóvedas</DropdownMenuLabel>}
        {files.vaults.map((v) => (
          <DropdownMenuItem key={v.path} onSelect={() => !v.current && post({ area: 'files', type: 'switch_vault', path: v.path })} className="group pr-1">
            <div className="min-w-0 flex-1">
              <div className={cn('truncate', v.current && 'font-semibold text-primary')}>{v.name}</div>
              {/* The start of a long path is the least telling part: it is the one cut. */}
              <div className="truncate text-left text-xs text-muted-foreground [direction:rtl]">
                <bdi>{v.path}</bdi>
              </div>
            </div>
            <button
              type="button"
              title="Quitar de la lista (no borra la carpeta)"
              aria-label={`Quitar ${v.name} de la lista`}
              onClick={(e) => {
                e.stopPropagation()
                post({ area: 'files', type: 'remove_vault', path: v.path })
              }}
              className="grid size-6 shrink-0 place-items-center rounded-sm text-muted-foreground opacity-0 group-focus:opacity-100 group-hover:opacity-100 hover:bg-background hover:text-foreground"
            >
              <X className="size-3.5" />
            </button>
          </DropdownMenuItem>
        ))}
        {files.vaults.length > 0 && <DropdownMenuSeparator />}
        <DropdownMenuItem onSelect={() => post({ area: 'files', type: 'pick_vault' })}>
          Abrir carpeta…<DropdownMenuShortcut>Ctrl+O</DropdownMenuShortcut>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

function NameField({ form, folders }: { form: Form; folders: Folders }) {
  const [name, setName] = useState('')
  const field = useRef<HTMLInputElement>(null)
  // Each time the field opens it starts empty and takes the keyboard.
  useEffect(() => {
    setName('')
    field.current?.focus()
  }, [form.id])
  const place = form.dir || 'la raíz de la bóveda'
  const cancel = () => post({ area: 'files', type: 'cancel_name' })
  const create = () => {
    const typed = name.trim()
    if (!typed) return
    if (form.folder) {
      // The new folder shows open, with the folders above it.
      folders.keep(form.dir ? `${form.dir}/${typed}` : typed)
      post({ area: 'files', type: 'create_folder', dir: form.dir, name })
    } else {
      post({ area: 'files', type: 'create_note', dir: form.dir, name })
    }
  }
  return (
    <div className="flex shrink-0 flex-col gap-1 border-b px-3 py-2">
      <label htmlFor="visor-nombre" className="text-xs text-muted-foreground">
        {form.folder ? `Nueva carpeta en ${place}` : `Nueva nota en ${place}`}
      </label>
      <input
        id="visor-nombre"
        ref={field}
        value={name}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter') create()
          else if (e.key === 'Escape') cancel()
          else return
          e.preventDefault()
        }}
        onBlur={() => !name && cancel()}
        placeholder={form.folder ? 'Nombre de la carpeta' : 'Nombre de la nota'}
        aria-invalid={!!form.error}
        aria-describedby={form.error ? 'visor-nombre-error' : undefined}
        className="h-7 rounded-md border bg-background px-2 text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-ring/50 aria-invalid:border-destructive"
      />
      {form.error && (
        <p id="visor-nombre-error" className="text-xs text-destructive">
          {form.error}
        </p>
      )}
      <p className="text-[11px] text-muted-foreground">Intro para crear, Esc para cancelar.</p>
    </div>
  )
}

/** The last notice, until it goes away by itself or is closed. */
function NoticeBox({ notice }: { notice: Notice | null }) {
  const [gone, setGone] = useState<number | null>(null)
  useEffect(() => {
    if (!notice) return
    const t = window.setTimeout(() => setGone(notice.id), notice.error ? 8000 : 4000)
    return () => window.clearTimeout(t)
  }, [notice])
  if (!notice || gone === notice.id) return null
  const elsewhere = notice.elsewhere
  return (
    <div
      role={notice.error ? 'alert' : 'status'}
      className={cn('mx-2 mb-2 flex shrink-0 items-start gap-2 rounded-lg bg-muted px-2.5 py-2 text-xs', notice.error && 'text-destructive')}
    >
      <div className="flex min-w-0 flex-1 flex-col items-start gap-1.5">
        <p className="break-words">{notice.text}</p>
        {elsewhere && (
          <Button variant="outline" size="xs" onClick={() => post({ area: 'files', type: 'open_elsewhere', path: elsewhere })}>
            Abrir con otra aplicación
          </Button>
        )}
      </div>
      <button type="button" title="Cerrar el aviso" onClick={() => setGone(notice.id)} className="text-muted-foreground hover:text-foreground">
        <X className="size-3.5" />
      </button>
    </div>
  )
}

/** The panel's right edge: dragging it changes the width, which Rust keeps. */
function Edge({ width, onDrag }: { width: number; onDrag: (w: number | null) => void }) {
  const start = useRef<{ x: number; w: number } | null>(null)
  const clamp = (w: number) => Math.round(Math.max(WIDTH.min, Math.min(WIDTH.max, w)))
  const move = (e: PointerEvent) => start.current && onDrag(clamp(start.current.w + e.clientX - start.current.x))
  const end = (e: PointerEvent) => {
    if (!start.current) return
    const w = clamp(start.current.w + e.clientX - start.current.x)
    start.current = null
    post({ area: 'files', type: 'width', width: w })
  }
  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label="Ancho del panel"
      title="Arrastra para cambiar el ancho"
      onPointerDown={(e) => {
        e.currentTarget.setPointerCapture(e.pointerId)
        start.current = { x: e.clientX, w: width }
      }}
      onPointerMove={move}
      onPointerUp={end}
      onPointerCancel={end}
      className="absolute inset-y-0 right-0 z-10 w-1.5 cursor-col-resize hover:bg-ring/30"
    />
  )
}

export function Panel() {
  const files = useArea('files')
  const tree = useArea('tree')
  const folders = useFolders(tree, files.current)
  // While the edge is dragged; Rust's width replaces it once it arrives.
  const [dragWidth, setDragWidth] = useState<number | null>(null)
  useEffect(() => setDragWidth(null), [files.width])
  if (!files.shown) return null
  const width = dragWidth ?? files.width ?? WIDTH.start
  const ask = (folder: boolean) => post({ area: 'files', type: 'ask_name', dir: null, folder })
  return (
    <aside style={{ width }} className="relative flex h-full shrink-0 flex-col border-r bg-sidebar">
      <div className="flex h-[30px] shrink-0 items-center gap-0.5 border-b px-1.5">
        <Vaults files={files} />
        <Button variant="ghost" size="icon-sm" className="text-muted-foreground" title="Nueva nota (Ctrl+N)" onClick={() => ask(false)}>
          <FilePlus2 />
        </Button>
        <Button variant="ghost" size="icon-sm" className="text-muted-foreground" title="Nueva carpeta" onClick={() => ask(true)}>
          <FolderPlus />
        </Button>
      </div>
      {files.form && <NameField form={files.form} folders={folders} />}
      <div className="flex items-center pt-2 pr-1.5 pb-1 pl-4">
        <span className="flex-1 text-[11px] font-semibold tracking-wide text-muted-foreground uppercase">Archivos</span>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="icon-xs" className="text-muted-foreground" title="Opciones del panel">
              <Ellipsis />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuCheckboxItem checked={files.show_other} onCheckedChange={(on) => post({ area: 'files', type: 'show_other', on })}>
              Mostrar archivos que visor no abre
            </DropdownMenuCheckboxItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      {tree.root ? (
        tree.root.dirs.length + tree.root.files.length > 0 ? (
          <Tree root={tree.root} folders={folders} current={files.current} />
        ) : (
          <p className="flex-1 px-4 py-2 text-xs text-muted-foreground italic">{tree.root.empty ?? 'Carpeta vacía'}</p>
        )
      ) : (
        <div className="flex flex-1 flex-col items-start gap-2 px-4 py-2 text-xs text-muted-foreground">
          <p>No hay ninguna bóveda abierta.</p>
          <Button variant="outline" size="xs" onClick={() => post({ area: 'files', type: 'pick_vault' })}>
            Abrir carpeta…
          </Button>
        </div>
      )}
      <NoticeBox notice={files.notice} />
      <Edge width={width} onDrag={setDragWidth} />
    </aside>
  )
}
