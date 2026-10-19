// D: B with the tree and panel header of A, rows packed closer (user, 2026-10-19).
import {
  ArrowLeft,
  ArrowRight,
  ChevronDown,
  ChevronRight,
  Folder,
  FolderOpen,
  Crosshair,
  FilePlus2,
  FolderPlus,
  Keyboard,
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

const icono = 'grid size-[22px] place-items-center rounded-[3px] text-muted-foreground hover:bg-accent hover:text-foreground'

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
          className="flex h-6 items-center gap-1.5 rounded-md pr-2 text-xs font-semibold text-muted-foreground hover:bg-muted hover:text-foreground"
        >
          <ChevronRight className={cn('size-3.5 shrink-0 transition-transform', nodo.abierta && 'rotate-90')} />
          <Icono className="size-3.5 shrink-0" />
          <span className="min-w-0 flex-1 overflow-hidden whitespace-nowrap [mask-image:linear-gradient(to_right,black_calc(100%-28px),transparent)]">{nodo.nombre}</span>
          <span className="ml-2 text-[11px] font-normal tabular-nums opacity-70">
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
        'flex h-6 items-center gap-2 rounded-md pr-2 text-[13px] text-foreground/90',
        actual ? 'bg-selection font-medium text-selection-foreground' : 'hover:bg-accent',
      )}
    >
      <IconoArchivo tipo={nodo.tipo} className={actual ? 'text-primary' : 'text-muted-foreground'} />
      <span className="min-w-0 flex-1 overflow-hidden whitespace-nowrap [mask-image:linear-gradient(to_right,black_calc(100%-28px),transparent)]">{nodo.nombre}</span>
    </div>
  )
}

function Lateral() {
  return (
    <aside className="flex h-full w-[264px] shrink-0 flex-col border-r bg-sidebar">
      <div className="flex h-[30px] shrink-0 items-center gap-0.5 border-b px-1.5">
        <Button variant="ghost" size="sm" className="h-6 min-w-0 flex-1 justify-start px-2 text-[13px] font-semibold" title="Cambiar de bóveda">
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
      <div className="px-4 pt-2 pb-1 text-[11px] font-semibold tracking-wide text-muted-foreground uppercase">
        Archivos
      </div>
      <ScrollArea className="min-h-0 flex-1">
        <div className="flex flex-col px-2 pb-3">
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
      <button className={item} title="Atajos de teclado (Ctrl+K)">
        <Keyboard className="size-3.5" />
      </button>
      <span className={cn(item, 'pr-3')}>{VERSION}</span>
    </footer>
  )
}

export default function VarianteD({ estado }: { estado: Estado }) {
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
