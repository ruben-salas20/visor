// Area C, the address row of a side: back, forward, reload, where the page
// is, and the annotation buttons. Still to come (phase 1): typing an
// address (Ctrl+L) and its error.
import { ArrowLeft, ArrowRight, ChevronRight, Crosshair, ListTree, RotateCw } from 'lucide-react'
import { cn } from '@/lib/utils'
import { post } from '@/protocol'
import { useArea } from '@/store'

const icon = 'grid size-[22px] place-items-center rounded-[3px] text-muted-foreground hover:bg-accent hover:text-foreground'

/** The path of the page, by parts, after the vault's name. */
function Crumbs({ target, vault }: { target: string; vault: string | null }) {
  if (!target) return <div className="min-w-0 flex-1 truncate px-1.5 text-muted-foreground">Nueva pestaña</div>
  if (/^https?:\/\//.test(target)) return <div className="min-w-0 flex-1 truncate px-1.5 text-muted-foreground">{target}</div>
  const loose = target.startsWith('/')
  const parts = (loose ? target.slice(1) : target).split('/')
  return (
    <div className="flex min-w-0 flex-1 items-center gap-0.5 truncate px-1.5 text-muted-foreground">
      {!loose && vault && <span>{vault}</span>}
      {parts.map((p, i) => (
        <span key={i} className="flex items-center gap-0.5">
          {(i > 0 || (!loose && vault)) && <ChevronRight className="size-3 opacity-60" />}
          <span className={i === parts.length - 1 ? 'text-foreground' : undefined}>{p}</span>
        </span>
      ))}
    </div>
  )
}

export function AddressRow({ side, focused }: { side: number; focused: boolean }) {
  const address = useArea('bar').sides.find((a) => a.side === side)
  const { vault } = useArea('files')
  if (!address) return <div className="h-[26px] shrink-0 border-b" />
  return (
    <div className={cn('flex h-[26px] shrink-0 items-center gap-0.5 border-b px-1.5', !focused && 'opacity-75')}>
      <button className={cn(icon, !address.back && 'opacity-40')} title="Atrás (Alt+←)" onClick={() => post({ area: 'bar', type: 'back', side })}>
        <ArrowLeft className="size-3.5" />
      </button>
      <button className={cn(icon, !address.forward && 'opacity-40')} title="Adelante (Alt+→)" onClick={() => post({ area: 'bar', type: 'forward', side })}>
        <ArrowRight className="size-3.5" />
      </button>
      <button className={icon} title="Recargar (Ctrl+R)" onClick={() => post({ area: 'bar', type: 'reload', side })}>
        <RotateCw className="size-3.5" />
      </button>
      <Crumbs target={address.target} vault={vault} />
      {address.annotations && (
        <>
          <button className={icon} title="Anotar un elemento (Ctrl+Shift+A)" onClick={() => post({ area: 'bar', type: 'pick_element', side })}>
            <Crosshair className="size-3.5" />
          </button>
          <button className={icon} title="Anotaciones de la página (Ctrl+Shift+L)" onClick={() => post({ area: 'bar', type: 'toggle_annotations', side })}>
            <ListTree className="size-3.5" />
          </button>
        </>
      )}
    </div>
  )
}
