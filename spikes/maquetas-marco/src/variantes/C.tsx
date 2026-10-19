// C: airy, notes-app style. Each side is a rounded card on the sidebar's color, the focused one
// with an accent ring. Toolbar first and pill tabs under it; the address is a centered pill;
// the find bar floats at the bottom of the page.
import {
  ArrowLeft,
  ArrowRight,
  ChevronRight,
  ChevronsUpDown,
  Crosshair,
  Folder,
  FolderPlus,
  ListTree,
  Plus,
  RotateCw,
  Settings,
  SquarePen,
  X,
} from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { ScrollArea } from '@/components/ui/scroll-area'
import { cn } from '@/lib/utils'
import { Ajustes, Buscar, IconoArchivo, Pagina, type Estado } from '../comunes'
import { ACTUAL, ARBOL, BOVEDA, LADOS, VERSION, type Lado, type Nodo } from '../datos'

function Rama({ nodo, nivel }: { nodo: Nodo; nivel: number }) {
  const sangria = { paddingLeft: 10 + nivel * 18 }
  if (nodo.tipo === 'carpeta') {
    return (
      <>
        <div style={sangria} className="flex h-8 items-center gap-2 rounded-lg pr-2 text-foreground/85 hover:bg-sidebar-accent">
          <ChevronRight className={cn('size-3.5 shrink-0 text-muted-foreground', nodo.abierta && 'rotate-90')} />
          <Folder className="size-4 shrink-0 text-muted-foreground" />
          <span className="truncate">{nodo.nombre}</span>
        </div>
        {nodo.abierta && nodo.hijos.map((h) => <Rama key={h.nombre} nodo={h} nivel={nivel + 1} />)}
      </>
    )
  }
  const actual = nodo.ruta === ACTUAL
  return (
    <div
      style={{ paddingLeft: 10 + nivel * 18 + 22 }}
      className={cn(
        'flex h-8 items-center gap-2 rounded-lg pr-2',
        actual ? 'bg-selection font-medium text-selection-foreground' : 'text-foreground/85 hover:bg-sidebar-accent',
      )}
    >
      <IconoArchivo tipo={nodo.tipo} className={cn('size-4', actual ? 'text-primary' : 'text-muted-foreground')} />
      <span className="truncate">{nodo.nombre}</span>
    </div>
  )
}

function Lateral({ estado }: { estado: Estado }) {
  return (
    <aside className="flex h-full w-[288px] shrink-0 flex-col px-3 pt-3 pb-2">
      <button className="flex items-center gap-3 rounded-xl p-2 text-left hover:bg-sidebar-accent" title="Cambiar de bóveda">
        <span className="grid size-9 place-items-center rounded-lg bg-primary text-base font-bold text-primary-foreground">C</span>
        <span className="flex min-w-0 flex-1 flex-col">
          <span className="truncate font-semibold">{BOVEDA}</span>
          <span className="text-xs text-muted-foreground">28 notas</span>
        </span>
        <ChevronsUpDown className="size-4 text-muted-foreground" />
      </button>
      <div className="mt-3 flex gap-2">
        <Button variant="outline" className="h-9 flex-1 justify-start rounded-lg bg-background" title="Nueva nota (Ctrl+N)">
          <SquarePen /> Nueva nota
        </Button>
        <Button variant="outline" size="icon" className="rounded-lg bg-background" title="Nueva carpeta">
          <FolderPlus />
        </Button>
      </div>
      <div className="mt-5 mb-1.5 px-2.5 text-xs font-medium text-muted-foreground">Notas</div>
      <ScrollArea className="-mx-1 min-h-0 flex-1">
        <div className="flex flex-col gap-0.5 px-1 pb-3">
          {ARBOL.map((n) => (
            <Rama key={n.nombre} nodo={n} nivel={0} />
          ))}
        </div>
      </ScrollArea>
      <Popover open={estado === 'ajustes'}>
        <PopoverTrigger asChild>
          <button
            className={cn(
              'flex h-9 items-center gap-2.5 rounded-lg px-2.5 text-foreground/85 hover:bg-sidebar-accent',
              estado === 'ajustes' && 'bg-sidebar-accent',
            )}
            title="Ajustes (Ctrl+,)"
          >
            <Settings className="size-4 text-muted-foreground" />
            Ajustes
            <span className="ml-auto text-xs text-muted-foreground">{VERSION}</span>
          </button>
        </PopoverTrigger>
        <PopoverContent side="right" align="end" sideOffset={14} className="w-80 rounded-xl p-5 text-[13px] shadow-xl">
          <Ajustes />
        </PopoverContent>
      </Popover>
    </aside>
  )
}

const redondo = 'grid size-8 place-items-center rounded-full text-muted-foreground hover:bg-accent hover:text-foreground'

function Panel({ lado, foco, buscar }: { lado: Lado; foco: boolean; buscar: boolean }) {
  const [carpeta, archivo] = (() => {
    const i = lado.direccion.lastIndexOf('/')
    return [lado.direccion.slice(0, i), lado.direccion.slice(i + 1)]
  })()
  const activa = lado.pestanas.find((t) => t.activa)!
  return (
    <section
      className={cn(
        'relative flex min-w-0 flex-1 flex-col overflow-hidden rounded-xl border bg-background',
        foco ? 'shadow-lg ring-2 ring-primary/70' : 'shadow-sm',
      )}
    >
      <div className="flex h-12 shrink-0 items-center gap-1 px-3">
        <button className={cn(redondo, !lado.atras && 'opacity-40')} title="Atrás (Alt+←)">
          <ArrowLeft className="size-4" />
        </button>
        <button className={cn(redondo, !lado.adelante && 'opacity-40')} title="Adelante (Alt+→)">
          <ArrowRight className="size-4" />
        </button>
        <button className={redondo} title="Recargar (Ctrl+R)">
          <RotateCw className="size-4" />
        </button>
        <div className="mx-3 flex h-8 min-w-0 flex-1 items-center justify-center gap-2 rounded-full bg-foreground/[0.07] px-4 text-[13px]">
          <IconoArchivo tipo={activa.tipo} className="text-muted-foreground" />
          <span className="truncate text-muted-foreground">{carpeta} /</span>
          <span className="shrink-0 font-medium">{archivo}</span>
        </div>
        <button className={redondo} title="Anotar un elemento (Ctrl+Shift+A)">
          <Crosshair className="size-4" />
        </button>
        <button className={redondo} title="Anotaciones de la página (Ctrl+Shift+L)">
          <ListTree className="size-4" />
        </button>
      </div>
      <div className="flex h-10 shrink-0 items-center gap-1.5 border-b px-3 pb-2">
        {lado.pestanas.map((t) => (
          <div
            key={t.titulo}
            title={t.titulo}
            className={cn(
              'group flex h-8 min-w-0 items-center gap-2 rounded-full px-3.5 text-[13px]',
              t.activa ? 'w-[190px]' : 'w-[148px]',
              t.activa
                ? cn('font-medium', foco ? 'bg-primary/15 text-foreground ring-1 ring-primary/40' : 'bg-secondary')
                : 'text-muted-foreground hover:bg-muted',
            )}
          >
            <IconoArchivo tipo={t.tipo} className={t.activa && foco ? 'text-primary' : 'opacity-70'} />
            <span className="truncate">{t.titulo}</span>
            {t.activa && <X className="-mr-1 size-3.5 shrink-0 opacity-60" />}
          </div>
        ))}
        <button className={cn(redondo, 'size-7')} title="Nueva pestaña (Ctrl+T)">
          <Plus className="size-4" />
        </button>
      </div>
      <div className="min-h-0 flex-1 px-14 py-10">
        <Pagina cual={lado.pagina} buscar={buscar} className="mx-auto max-w-[600px] text-[16px]" />
      </div>
      {buscar && (
        <Buscar className="absolute inset-x-6 bottom-5 rounded-full border bg-popover py-1.5 pr-2 pl-3 shadow-xl [&>div:first-child]:rounded-full" />
      )}
    </section>
  )
}

export default function VarianteC({ estado }: { estado: Estado }) {
  return (
    <div className="flex h-full bg-sidebar text-[14px]">
      <Lateral estado={estado} />
      <main className="flex min-w-0 flex-1 gap-2.5 py-2.5 pr-2.5">
        {LADOS.map((l, i) => (
          <Panel key={i} lado={l} foco={i === 0} buscar={estado === 'buscar' && i === 0} />
        ))}
      </main>
    </div>
  )
}
