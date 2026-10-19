import { ChevronRight, Folder, FileText } from 'lucide-react'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible'
import { ScrollArea } from '@/components/ui/scroll-area'
import { cn } from '@/lib/utils'
import { acciones } from './marco'

// Groups consecutive "carpeta/archivo" paths by folder, keeping the order Rust sends.
function agrupar(lista: string[]) {
  const grupos: { dir: string; archivos: { ruta: string; nombre: string }[] }[] = []
  for (const ruta of lista) {
    const [dir, nombre] = ruta.split('/')
    const ultimo = grupos.at(-1)
    if (ultimo && ultimo.dir === dir) ultimo.archivos.push({ ruta, nombre })
    else grupos.push({ dir, archivos: [{ ruta, nombre }] })
  }
  return grupos
}

export function Arbol({ lista }: { lista: string[] }) {
  return (
    <ScrollArea
      className="min-h-0 flex-1"
      onMouseDown={(e) => {
        // Remember what is under the pointer; the drop check runs on mouseup (see marco.ts).
        const item = (e.target as Element).closest<HTMLElement>('.item')
        acciones.pendienteAl(item?.dataset.ruta ?? null)
      }}
    >
      <nav className="flex flex-col gap-1 p-2 pb-4">
        {agrupar(lista).map((g) => (
          <Collapsible key={g.dir} defaultOpen>
            <CollapsibleTrigger className="group flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-xs font-semibold text-muted-foreground transition-colors hover:bg-accent/60 hover:text-foreground">
              <ChevronRight className="size-3.5 shrink-0 transition-transform duration-200 group-data-[state=open]:rotate-90" />
              <Folder className="size-3.5 shrink-0" />
              <span className="truncate">{g.dir}</span>
              <span className="ml-auto font-normal tabular-nums">{g.archivos.length}</span>
            </CollapsibleTrigger>
            <CollapsibleContent className="flex flex-col gap-px pt-0.5 pb-1 pl-4">
              {g.archivos.map(({ ruta, nombre }) => (
                <div
                  key={ruta}
                  className={cn(
                    'item flex min-w-0 cursor-pointer select-none items-center gap-2 rounded-md px-2 py-1 text-[13px] text-foreground/90',
                    'transition-colors hover:bg-accent hover:text-accent-foreground active:bg-accent/80',
                  )}
                  data-ruta={ruta}
                  draggable
                  title={ruta}
                  onDragStart={(e) => {
                    e.dataTransfer.setData('text/plain', ruta)
                    e.dataTransfer.effectAllowed = 'copy'
                  }}
                  onClick={() => acciones.abrirDesdeArbol(ruta)}
                >
                  <FileText className="size-3.5 shrink-0 text-muted-foreground" />
                  <span className="truncate">{nombre}</span>
                </div>
              ))}
            </CollapsibleContent>
          </Collapsible>
        ))}
      </nav>
    </ScrollArea>
  )
}
