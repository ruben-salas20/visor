// Area C, the address row of a side: back, forward, reload, where the page
// is, the address field (Ctrl+L) and the annotation buttons.
import { tr } from '@/i18n'
import { useEffect, useRef, useState } from 'react'
import { ArrowLeft, ArrowRight, ChevronRight, Crosshair, ListTree, RotateCw } from 'lucide-react'
import { cn } from '@/lib/utils'
import { post } from '@/protocol'
import { useArea } from '@/store'

const icon = 'grid size-[22px] shrink-0 place-items-center rounded-[3px] text-muted-foreground hover:bg-accent hover:text-foreground'

/** The path of the page, by parts, after the vault's name. The earlier parts shrink first. */
function Crumbs({ target, vault }: { target: string; vault: string | null }) {
  if (!target) return <div className="min-w-0 flex-1 truncate px-1.5 text-muted-foreground">Nueva pestaña</div>
  if (/^https?:\/\//.test(target)) return <div className="min-w-0 flex-1 truncate px-1.5 text-muted-foreground">{target}</div>
  const loose = target.startsWith('/')
  const parts = (loose ? target.slice(1) : target).split('/')
  return (
    <div className="flex min-w-0 flex-1 items-center gap-0.5 overflow-hidden px-1.5 text-muted-foreground">
      {!loose && vault && <span className="min-w-0 truncate">{vault}</span>}
      {parts.map((p, i) => {
        const last = i === parts.length - 1
        return (
          <span key={i} className="flex min-w-0 items-center gap-0.5">
            {(i > 0 || (!loose && vault)) && <ChevronRight className="size-3 shrink-0 opacity-60" />}
            <span className={cn('min-w-0 truncate', last ? 'shrink-[0.01] text-foreground' : 'shrink')}>{p}</span>
          </span>
        )
      })}
    </div>
  )
}

export function AddressRow({ side, focused }: { side: number; focused: boolean }) {
  const address = useArea('bar').sides.find((a) => a.side === side)
  const { vault } = useArea('files')
  const edit = address?.edit ?? 0
  const refused = address?.refused ?? 0
  const [seenEdit, setSeenEdit] = useState(edit)
  const [seenRefused, setSeenRefused] = useState(refused)
  const [typing, setTyping] = useState(false)
  const [text, setText] = useState('')
  const [marked, setMarked] = useState(false)
  const field = useRef<HTMLInputElement>(null)

  // Ctrl+L gives the frame's view the keyboard focus after the field opens,
  // and WebKit then hands the focus back to what had it before: take it back.
  useEffect(() => {
    if (!typing) return
    const again = () => field.current?.focus()
    window.addEventListener('focus', again)
    return () => window.removeEventListener('focus', again)
  }, [typing])

  // Rust counts each request: a new count opens the field, or marks it when
  // the address led nowhere. It is set while rendering so the field opens
  // already showing the path, with nothing to flash first.
  if (edit !== seenEdit) {
    setSeenEdit(edit)
    setText(address?.target ?? '')
    setTyping(true)
    setMarked(false)
  }
  if (refused !== seenRefused) {
    setSeenRefused(refused)
    setTyping(true)
    setMarked(true)
  }

  if (!address) return <div className="h-[26px] shrink-0 border-b" />
  const leave = () => {
    setTyping(false)
    setMarked(false)
  }
  const go = () => {
    post({ area: 'bar', type: 'go', side, text })
    setTyping(false)
  }
  return (
    <div className={cn('flex h-[26px] shrink-0 items-center gap-0.5 border-b px-1.5', !focused && 'opacity-75')}>
      <button className={cn(icon, !address.back && 'opacity-40')} title={tr('Atrás (Alt+←)', 'Back (Alt+←)')} onClick={() => post({ area: 'bar', type: 'back', side })}>
        <ArrowLeft className="size-3.5" />
      </button>
      <button className={cn(icon, !address.forward && 'opacity-40')} title={tr('Adelante (Alt+→)', 'Forward (Alt+→)')} onClick={() => post({ area: 'bar', type: 'forward', side })}>
        <ArrowRight className="size-3.5" />
      </button>
      <button className={icon} title={tr('Recargar (Ctrl+R)', 'Reload (Ctrl+R)')} onClick={() => post({ area: 'bar', type: 'reload', side })}>
        <RotateCw className="size-3.5" />
      </button>
      {typing ? (
        <input
          key={edit}
          autoFocus
          spellCheck={false}
          aria-label={tr('Dirección', 'Address')}
          aria-invalid={marked}
          placeholder={tr('Ruta, dirección web o búsqueda (Ctrl+L)', 'Path, web address or search (Ctrl+L)')}
          value={text}
          onChange={(e) => {
            setText(e.target.value)
            setMarked(false)
          }}
          onFocus={(e) => e.currentTarget.select()}
          ref={field}
          // A blur that comes with the view getting focus is WebKit's, not the
          // person's: only a blur that leaves the focus elsewhere closes the field.
          onBlur={() =>
            requestAnimationFrame(() => {
              if (document.activeElement !== field.current) leave()
            })
          }
          onKeyDown={(e) => {
            if (e.key === 'Enter') go()
            else if (e.key === 'Escape') {
              leave()
              post({ area: 'bar', type: 'leave', side })
            }
          }}
          className={cn(
            'h-[22px] min-w-0 flex-1 rounded-[3px] border bg-background px-1.5 text-foreground outline-none',
            marked ? 'border-destructive' : 'border-ring',
          )}
        />
      ) : (
        <div
          className="flex min-w-0 flex-1 cursor-text"
          title={tr('Escribe una dirección (Ctrl+L)', 'Type an address (Ctrl+L)')}
          onClick={() => {
            setText(address.target)
            setMarked(false)
            setTyping(true)
          }}
        >
          <Crumbs target={address.target} vault={vault} />
        </div>
      )}
      {address.annotations && (
        <>
          <button className={icon} title={tr('Anotar un elemento (Ctrl+Shift+A)', 'Annotate an element (Ctrl+Shift+A)')} onClick={() => post({ area: 'bar', type: 'pick_element', side })}>
            <Crosshair className="size-3.5" />
          </button>
          <button className={icon} title={tr('Anotaciones de la página (Ctrl+Shift+L)', 'Page annotations (Ctrl+Shift+L)')} onClick={() => post({ area: 'bar', type: 'toggle_annotations', side })}>
            <ListTree className="size-3.5" />
          </button>
        </>
      )}
    </div>
  )
}
