import { FileText, Plus, X } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import type { Panel } from './global'
import { acciones } from './marco'

function PestanaFila({ lado, id, ruta, activa }: { lado: number; id: number; ruta: string; activa: boolean }) {
  return (
    <div
      className={cn(
        'pestana group flex h-8 min-w-0 max-w-[14rem] shrink-0 cursor-pointer items-center gap-1.5 rounded-t-lg border border-b-0 pr-1 pl-2.5 text-xs transition-colors',
        activa
          ? 'activa border-border bg-card font-medium text-foreground shadow-[0_-1px_0_0_var(--color-primary)_inset]'
          : 'border-transparent text-muted-foreground hover:bg-accent/50 hover:text-foreground',
      )}
      data-lado={lado}
      data-id={id}
      title={ruta}
      onClick={() => acciones.activar(lado, id)}
    >
      <FileText className="size-3.5 shrink-0 opacity-70" />
      <span className="truncate">{ruta.split('/').pop()}</span>
      <button
        type="button"
        className="cerrar ml-0.5 grid size-5 shrink-0 place-items-center rounded-sm text-muted-foreground opacity-60 transition hover:bg-destructive/15 hover:text-destructive hover:opacity-100"
        data-lado={lado}
        data-id={id}
        aria-label="Cerrar pestaña"
        onClick={(e) => {
          // The close button sits inside the tab: stop the click from also activating the tab.
          e.stopPropagation()
          acciones.cerrar(lado, id)
        }}
      >
        <X className="size-3" />
      </button>
    </div>
  )
}

export function Paneles({ paneles, foco }: { paneles: Panel[]; foco: number }) {
  return paneles.map((p, lado) => (
    <section
      key={lado}
      className={cn(
        'panel flex min-w-0 flex-1 flex-col overflow-hidden rounded-xl border bg-card shadow-sm transition-[box-shadow,border-color] duration-200',
        foco === lado && 'foco border-primary/40 ring-3 ring-primary/15',
      )}
      data-lado={lado}
    >
      <div className="pestanas flex h-10 shrink-0 items-end gap-1 overflow-x-auto border-b bg-muted/60 px-2 pt-1.5">
        {p.pestanas.map((t) => (
          <PestanaFila key={t.id} lado={lado} id={t.id} ruta={t.ruta} activa={t.activa} />
        ))}
        <Button
          variant="ghost"
          size="sm"
          className="nueva mb-1 h-7 shrink-0 gap-1 text-muted-foreground"
          data-lado={lado}
          onClick={() => acciones.nuevaPestana(lado)}
        >
          <Plus className="size-3.5" />
          nueva pestaña
        </Button>
      </div>
      {/* Rust puts a page webview over this empty, transparent box. Keep it empty. */}
      <div className="contenido min-h-0 flex-1" data-lado={lado} />
    </section>
  ))
}
