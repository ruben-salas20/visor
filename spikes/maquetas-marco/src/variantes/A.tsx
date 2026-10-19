// A: the Svelte prototype (spikes/u1-marco-html/marcos/svelte) carried over to React:
// browser-like rounded tabs over a muted strip, a 2 px accent line under the focused side.
import {
  ArrowLeft,
  ArrowRight,
  ChevronDown,
  ChevronRight,
  Crosshair,
  FilePlus2,
  Folder,
  FolderOpen,
  FolderPlus,
  ListTree,
  Plus,
  RotateCw,
  Settings,
  X,
} from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { ScrollArea } from '@/components/ui/scroll-area'
import { cn } from '@/lib/utils'
import { Ajustes, Buscar, IconoArchivo, Pagina, type Estado } from '../comunes'
import { ACTUAL, ARBOL, BOVEDA, LADOS, VERSION, type Lado, type Nodo } from '../datos'

const CUENTA: Record<string, number> = {
  'Tema 1 · Límites y continuidad': 7,
  'Tema 2 · Derivadas parciales': 5,
  'Tema 4 · Green y Stokes': 4,
}

function Rama({ nodo, nivel }: { nodo: Nodo; nivel: number }) {
  const sangria = { paddingLeft: 8 + nivel * 14 }
  if (nodo.tipo === 'carpeta') {
    const Icono = nodo.abierta ? FolderOpen : Folder
    return (
      <>
        <div
          style={sangria}
          className="flex h-7 items-center gap-1.5 rounded-md pr-2 text-xs font-semibold text-muted-foreground hover:bg-muted hover:text-foreground"
        >
          <ChevronRight className={cn('size-3.5 shrink-0 transition-transform', nodo.abierta && 'rotate-90')} />
          <Icono className="size-3.5 shrink-0" />
          <span className="truncate">{nodo.nombre}</span>
          <span className="ml-auto text-[11px] font-normal tabular-nums opacity-70">
            {CUENTA[nodo.nombre] ?? nodo.hijos.length}
          </span>
        </div>
        {nodo.abierta && nodo.hijos.map((h) => <Rama key={h.nombre} nodo={h} nivel={nivel + 1} />)}
      </>
    )
  }
  const actual = nodo.ruta === ACTUAL
  return (
    <div
      style={{ paddingLeft: 8 + nivel * 14 + 20 }}
      className={cn(
        'flex h-7 items-center gap-2 rounded-md pr-2 text-[13px] text-foreground/90',
        actual ? 'bg-selection font-medium text-selection-foreground' : 'hover:bg-accent',
      )}
    >
      <IconoArchivo tipo={nodo.tipo} className={actual ? 'text-primary' : 'text-muted-foreground'} />
      <span className="truncate">{nodo.nombre}</span>
    </div>
  )
}

function Lateral({ estado }: { estado: Estado }) {
  return (
    <aside className="flex h-full w-[264px] shrink-0 flex-col border-r bg-sidebar">
      <div className="flex h-11 shrink-0 items-center gap-0.5 border-b px-2">
        <Button variant="ghost" size="sm" className="min-w-0 flex-1 justify-start px-2 font-semibold" title="Cambiar de bóveda">
          <span className="truncate">{BOVEDA}</span>
          <ChevronDown className="size-3.5 text-muted-foreground" />
        </Button>
        <Button variant="ghost" size="icon-sm" className="text-muted-foreground" title="Nueva nota (Ctrl+N)">
          <FilePlus2 />
        </Button>
        <Button variant="ghost" size="icon-sm" className="text-muted-foreground" title="Nueva carpeta">
          <FolderPlus />
        </Button>
      </div>
      <div className="px-4 pt-3 pb-1.5 text-[11px] font-semibold tracking-wide text-muted-foreground uppercase">
        Archivos
      </div>
      <ScrollArea className="min-h-0 flex-1">
        <div className="flex flex-col gap-px px-2 pb-3">
          {ARBOL.map((n) => (
            <Rama key={n.nombre} nodo={n} nivel={0} />
          ))}
        </div>
      </ScrollArea>
      <div className="flex h-10 shrink-0 items-center justify-between border-t px-2">
        <Popover open={estado === 'ajustes'}>
          <PopoverTrigger asChild>
            <Button
              variant="ghost"
              size="icon-sm"
              className={cn('text-muted-foreground', estado === 'ajustes' && 'bg-accent text-foreground')}
              title="Ajustes (Ctrl+,)"
            >
              <Settings />
            </Button>
          </PopoverTrigger>
          <PopoverContent side="top" align="start" sideOffset={6} className="w-80 shadow-xl">
            <Ajustes />
          </PopoverContent>
        </Popover>
        <span className="pr-2 text-xs text-muted-foreground">{VERSION}</span>
      </div>
    </aside>
  )
}

function Panel({ lado, foco, buscar }: { lado: Lado; foco: boolean; buscar: boolean }) {
  return (
    <section className="flex min-w-0 flex-1 flex-col bg-background">
      <div
        className={cn(
          'flex h-10 shrink-0 items-end gap-1 overflow-hidden border-b bg-muted/60 px-2 pt-1.5',
          foco && 'shadow-[inset_0_-2px_0_var(--t-accent)]',
        )}
      >
        {lado.pestanas.map((t) => (
          <div
            key={t.titulo}
            title={t.titulo}
            className={cn(
              'group flex h-8 w-[172px] min-w-0 items-center gap-2 rounded-t-lg border border-b-0 px-3 text-xs',
              t.activa
                ? 'border-border bg-card font-medium text-foreground'
                : 'border-transparent text-muted-foreground hover:bg-background/70',
            )}
          >
            <IconoArchivo tipo={t.tipo} className={t.activa && foco ? 'text-primary' : 'opacity-70'} />
            <span className="truncate">{t.titulo}</span>
            <X className={cn('-mr-1 size-3 shrink-0', t.activa ? 'opacity-70' : 'opacity-0 group-hover:opacity-60')} />
          </div>
        ))}
        <Button variant="ghost" size="icon-xs" className="mb-1 text-muted-foreground" title="Nueva pestaña (Ctrl+T)">
          <Plus />
        </Button>
      </div>
      <div className="flex h-10 shrink-0 items-center gap-0.5 border-b bg-card px-2">
        <Button variant="ghost" size="icon-sm" disabled={!lado.atras} title="Atrás (Alt+←)">
          <ArrowLeft />
        </Button>
        <Button variant="ghost" size="icon-sm" disabled={!lado.adelante} title="Adelante (Alt+→)">
          <ArrowRight />
        </Button>
        <Button variant="ghost" size="icon-sm" className="text-muted-foreground" title="Recargar (Ctrl+R)">
          <RotateCw />
        </Button>
        <div className="mx-1.5 flex h-7 min-w-0 flex-1 items-center rounded-md border bg-background px-2.5 text-[13px] text-foreground/90">
          <span className="truncate">{lado.direccion}</span>
        </div>
        <Button variant="ghost" size="icon-sm" className="text-muted-foreground" title="Anotar un elemento (Ctrl+Shift+A)">
          <Crosshair />
        </Button>
        <Button variant="ghost" size="icon-sm" className="text-muted-foreground" title="Anotaciones de la página (Ctrl+Shift+L)">
          <ListTree />
        </Button>
      </div>
      {buscar && <Buscar className="h-10 shrink-0 border-b bg-card px-3" />}
      <div className="min-h-0 flex-1 px-10 py-8">
        <Pagina cual={lado.pagina} buscar={buscar} className="mx-auto max-w-[620px] text-[15px]" />
      </div>
    </section>
  )
}

export default function VarianteA({ estado }: { estado: Estado }) {
  return (
    <div className="flex h-full">
      <Lateral estado={estado} />
      <main className="flex min-w-0 flex-1 divide-x divide-border">
        {LADOS.map((l, i) => (
          <Panel key={i} lado={l} foco={i === 0} buscar={estado === 'buscar' && i === 0} />
        ))}
      </main>
    </div>
  )
}
