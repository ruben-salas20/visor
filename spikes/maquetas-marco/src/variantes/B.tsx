// B: dense, code-editor style. Flat full-height tabs with a top accent line, a breadcrumb
// instead of an address box, indent guides in the tree, a status bar that holds the settings,
// and the find bar floating over the page.
import {
  ArrowLeft,
  ArrowRight,
  ChevronDown,
  ChevronRight,
  Crosshair,
  FilePlus2,
  FolderPlus,
  ListTree,
  Plus,
  RotateCw,
  Settings,
  X,
} from 'lucide-react'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { ScrollArea } from '@/components/ui/scroll-area'
import { cn } from '@/lib/utils'
import { Ajustes, Buscar, IconoArchivo, Pagina, type Estado } from '../comunes'
import { ACTUAL, ARBOL, BOVEDA, LADOS, VERSION, type Lado, type Nodo } from '../datos'

const icono = 'grid size-[22px] place-items-center rounded-[3px] text-muted-foreground hover:bg-accent hover:text-foreground'

function Guias({ nivel }: { nivel: number }) {
  return (
    <>
      {Array.from({ length: nivel }, (_, i) => (
        <span
          key={i}
          className="h-[22px] w-3 shrink-0"
          style={{ marginLeft: i === 0 ? 7 : 0, borderLeft: '1px solid var(--border)' }}
        />
      ))}
    </>
  )
}

function Rama({ nodo, nivel }: { nodo: Nodo; nivel: number }) {
  if (nodo.tipo === 'carpeta') {
    return (
      <>
        <div className="flex h-[22px] items-center gap-1 pr-2 hover:bg-accent">
          <Guias nivel={nivel} />
          <ChevronRight className={cn('ml-1 size-3.5 shrink-0 text-muted-foreground', nodo.abierta && 'rotate-90')} />
          <span className="truncate">{nodo.nombre}</span>
        </div>
        {nodo.abierta && nodo.hijos.map((h) => <Rama key={h.nombre} nodo={h} nivel={nivel + 1} />)}
      </>
    )
  }
  const actual = nodo.ruta === ACTUAL
  return (
    <div
      className={cn(
        'flex h-[22px] items-center gap-1.5 pr-2',
        actual ? 'bg-selection text-selection-foreground outline outline-1 -outline-offset-1 outline-primary/70' : 'hover:bg-accent',
      )}
    >
      <Guias nivel={nivel} />
      <span className="ml-1 w-3.5 shrink-0" />
      <IconoArchivo tipo={nodo.tipo} className={cn('size-3', nodo.tipo === 'md' ? 'text-primary/80' : 'text-muted-foreground')} />
      <span className="truncate">{nodo.nombre}</span>
    </div>
  )
}

function Lateral() {
  return (
    <aside className="flex h-full w-[248px] shrink-0 flex-col border-r bg-sidebar">
      <div className="flex h-[30px] shrink-0 items-center gap-0.5 pr-1.5 pl-3">
        <button className="flex min-w-0 flex-1 items-center gap-1 text-[11px] font-bold tracking-wider uppercase" title="Cambiar de bóveda">
          <span className="truncate">{BOVEDA}</span>
          <ChevronDown className="size-3 text-muted-foreground" />
        </button>
        <button className={icono} title="Nueva nota (Ctrl+N)">
          <FilePlus2 className="size-3.5" />
        </button>
        <button className={icono} title="Nueva carpeta">
          <FolderPlus className="size-3.5" />
        </button>
      </div>
      <ScrollArea className="min-h-0 flex-1">
        <div className="flex flex-col pb-2 pl-1">
          {ARBOL.map((n) => (
            <Rama key={n.nombre} nodo={n} nivel={0} />
          ))}
        </div>
      </ScrollArea>
    </aside>
  )
}

function Miga({ ruta }: { ruta: string }) {
  const partes = ruta.split('/')
  return (
    <div className="flex min-w-0 flex-1 items-center gap-0.5 truncate px-1.5 text-muted-foreground">
      <span>{BOVEDA}</span>
      {partes.map((p, i) => (
        <span key={i} className="flex items-center gap-0.5">
          <ChevronRight className="size-3 opacity-60" />
          <span className={i === partes.length - 1 ? 'text-foreground' : undefined}>{p}</span>
        </span>
      ))}
    </div>
  )
}

function Panel({ lado, foco, buscar }: { lado: Lado; foco: boolean; buscar: boolean }) {
  return (
    <section className="flex min-w-0 flex-1 flex-col bg-background">
      <div className="flex h-[30px] shrink-0 items-stretch bg-sidebar">
        {lado.pestanas.map((t) => (
          <div
            key={t.titulo}
            title={t.titulo}
            className={cn(
              'group relative flex w-[178px] min-w-0 items-center gap-1.5 border-r pr-2 pl-3',
              t.activa ? 'bg-background text-foreground' : 'border-b text-muted-foreground',
              t.activa && foco && 'before:absolute before:inset-x-0 before:top-0 before:h-0.5 before:bg-primary',
            )}
          >
            <IconoArchivo tipo={t.tipo} className={cn('size-3', t.activa && foco ? 'text-primary' : 'opacity-70')} />
            <span className={cn('truncate', !foco && 'opacity-80')}>{t.titulo}</span>
            <X className={cn('size-3 shrink-0', t.activa ? 'opacity-80' : 'opacity-0 group-hover:opacity-70')} />
          </div>
        ))}
        <div className="flex flex-1 items-center border-b pl-1">
          <button className={icono} title="Nueva pestaña (Ctrl+T)">
            <Plus className="size-3.5" />
          </button>
        </div>
      </div>
      <div className={cn('flex h-[26px] shrink-0 items-center gap-0.5 border-b px-1.5', !foco && 'opacity-75')}>
        <button className={cn(icono, !lado.atras && 'opacity-40')} title="Atrás (Alt+←)">
          <ArrowLeft className="size-3.5" />
        </button>
        <button className={cn(icono, !lado.adelante && 'opacity-40')} title="Adelante (Alt+→)">
          <ArrowRight className="size-3.5" />
        </button>
        <button className={icono} title="Recargar (Ctrl+R)">
          <RotateCw className="size-3.5" />
        </button>
        <Miga ruta={lado.direccion} />
        <button className={icono} title="Anotar un elemento (Ctrl+Shift+A)">
          <Crosshair className="size-3.5" />
        </button>
        <button className={icono} title="Anotaciones de la página (Ctrl+Shift+L)">
          <ListTree className="size-3.5" />
        </button>
      </div>
      <div className="relative min-h-0 flex-1 px-8 py-6">
        {buscar && (
          <Buscar
            compacto
            className="absolute top-0 right-4 z-10 rounded-b-md border border-t-0 bg-popover px-2 py-1 shadow-lg"
          />
        )}
        <Pagina cual={lado.pagina} buscar={buscar} className="max-w-[640px] text-[15px]" />
      </div>
    </section>
  )
}

function Estado({ estado }: { estado: Estado }) {
  const item = 'flex h-full items-center gap-1 px-2 hover:bg-accent'
  return (
    <footer className="flex h-6 shrink-0 items-center border-t bg-sidebar text-[11px] text-muted-foreground">
      <Popover open={estado === 'ajustes'}>
        <PopoverTrigger asChild>
          <button className={cn(item, 'pl-3', estado === 'ajustes' && 'bg-accent text-foreground')} title="Ajustes (Ctrl+,)">
            <Settings className="size-3.5" /> Ajustes
          </button>
        </PopoverTrigger>
        <PopoverContent side="top" align="start" sideOffset={6} className="w-80 text-xs">
          <Ajustes />
        </PopoverContent>
      </Popover>
      <span className={item}>{BOVEDA}</span>
      <span className="ml-auto" />
      <span className={item}>Markdown</span>
      <span className={item}>Zoom 110 %</span>
      <span className={cn(item, 'pr-3')}>{VERSION}</span>
    </footer>
  )
}

export default function VarianteB({ estado }: { estado: Estado }) {
  return (
    <div className="flex h-full flex-col text-[12px]">
      <div className="flex min-h-0 flex-1">
        <Lateral />
        <main className="flex min-w-0 flex-1 divide-x divide-border">
          {LADOS.map((l, i) => (
            <Panel key={i} lado={l} foco={i === 0} buscar={estado === 'buscar' && i === 0} />
          ))}
        </main>
      </div>
      <Estado estado={estado} />
    </div>
  )
}
