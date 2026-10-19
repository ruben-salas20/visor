// Area B, the sides: each one's tab row, its address row (area C) and the
// hole where Rust puts its pages. Between two sides, the divider. The drop
// zones over the pages and page fullscreen are Rust's (src/ui/host.rs): the
// pages lie over the frame, so the frame never sees what passes over them.
import { tr } from '@/i18n'
import { Plus, X } from 'lucide-react'
import { lazy, Suspense, useEffect, useRef, useState } from 'react'
import { AddressRow } from '@/areas/bar/AddressRow'
import { KindIcon } from '@/components/kind-icon'
import { useHole } from '@/holes'
import { setDragging } from '@/overlay'
import { cn } from '@/lib/utils'
import { post } from '@/protocol'
import { useArea } from '@/store'
import type { Side as SideState } from './protocol'

// The find bar loads the first time it opens (area D).
const Find = lazy(() => import('@/areas/find/Find'))

const icon = 'grid size-[22px] place-items-center rounded-[3px] text-muted-foreground hover:bg-accent hover:text-foreground'

function TabRow({ side, focused }: { side: SideState; focused: boolean }) {
  const strip = useRef<HTMLDivElement>(null)
  const activeId = side.tabs[side.active]?.id
  useEffect(() => {
    const el = strip.current
    if (!el) return
    const reveal = () => el.querySelector('[data-active]')?.scrollIntoView({ block: 'nearest', inline: 'nearest' })
    reveal()
    // The row narrows when the window or the divider moves.
    const resized = new ResizeObserver(reveal)
    resized.observe(el)
    return () => resized.disconnect()
  }, [activeId, side.tabs.length])
  return (
    <div className="flex h-[30px] shrink-0 items-stretch bg-sidebar select-none">
      {/* The tabs share the width down to a minimum; past it, the row scrolls. */}
      <div
        ref={strip}
        className="flex min-w-0 items-stretch overflow-x-auto overflow-y-hidden [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
        onWheel={(e) => {
          if (strip.current && e.deltaY && !e.deltaX) strip.current.scrollLeft += e.deltaY
        }}
      >
        {side.tabs.map((t, i) => {
          const active = i === side.active
          return (
            <div
              key={t.id}
              data-active={active || undefined}
              title={t.target || tr('Nueva pestaña', 'New tab')}
              onClick={() => post({ area: 'sides', type: 'select', tab: t.id })}
              // Without this, the middle button starts WebKit's autoscroll.
              onMouseDown={(e) => e.button === 1 && e.preventDefault()}
              onAuxClick={(e) => e.button === 1 && post({ area: 'sides', type: 'close', tab: t.id })}
              className={cn(
                'group relative flex min-w-[88px] max-w-[178px] flex-[1_1_178px] items-center gap-1.5 border-r pr-2 pl-3',
                active ? 'bg-background text-foreground' : 'border-b text-muted-foreground',
                active && focused && 'before:absolute before:inset-x-0 before:top-0 before:h-0.5 before:bg-primary',
              )}
            >
              <KindIcon kind={t.kind} className={cn('size-3 shrink-0', active && focused ? 'text-primary' : 'opacity-70')} />
              <span className={cn('min-w-0 flex-1 truncate', !focused && 'opacity-80')}>{t.label}</span>
              <button
                title={tr('Cerrar pestaña (Ctrl+W)', 'Close tab (Ctrl+W)')}
                onClick={(e) => {
                  e.stopPropagation()
                  post({ area: 'sides', type: 'close', tab: t.id })
                }}
                className={cn('shrink-0', active ? 'opacity-80' : 'opacity-0 group-hover:opacity-70')}
              >
                <X className="size-3" />
              </button>
            </div>
          )
        })}
      </div>
      <div className="flex min-w-[30px] flex-1 items-center border-b pl-1">
        <button className={icon} title={tr('Nueva pestaña (Ctrl+T)', 'New tab (Ctrl+T)')} onClick={() => post({ area: 'sides', type: 'new', side: side.id })}>
          <Plus className="size-3.5" />
        </button>
      </div>
    </div>
  )
}

function Side({ side, focused, finding, grow }: { side: SideState; focused: boolean; finding: boolean; grow: number }) {
  const hole = useHole(side.id)
  const focus = () => !focused && post({ area: 'sides', type: 'focus', side: side.id })
  return (
    <section className="flex min-w-0 flex-col" style={{ flex: `${grow} 1 0` }} onMouseDown={focus}>
      <div className="bg-background">
        <TabRow side={side} focused={focused} />
        <AddressRow side={side.id} focused={focused} />
      </div>
      {/* Transparent: this side's page views lie under it. The find bar lies over them, at the top. */}
      <div className="relative flex min-h-0 flex-1">
        <div ref={hole} className="min-h-0 flex-1" />
        {finding && (
          <Suspense fallback={null}>
            <Find />
          </Suspense>
        )}
      </div>
    </section>
  )
}

const clamp = (r: number) => Math.min(0.9, Math.max(0.1, r))

/** Drags the share of the left side. The pointer stays with the divider over
 * the pages: the frame takes the whole window while it moves. */
function Divider({ main, onMove }: { main: React.RefObject<HTMLElement | null>; onMove: (r: number) => void }) {
  const at = (x: number) => {
    const r = main.current?.getBoundingClientRect()
    return r && r.width > 0 ? clamp((x - r.left) / r.width) : null
  }
  return (
    <div
      role="separator"
      aria-orientation="vertical"
      title={tr('Arrastra para cambiar el ancho de los lados', 'Drag to change the width of the sides')}
      className="group relative w-[5px] shrink-0 cursor-col-resize touch-none bg-background"
      onPointerDown={(e) => {
        if (e.button !== 0) return
        e.preventDefault()
        e.currentTarget.setPointerCapture(e.pointerId)
        setDragging(true)
      }}
      onLostPointerCapture={() => setDragging(false)}
      onPointerMove={(e) => {
        const r = at(e.clientX)
        if (r !== null && e.currentTarget.hasPointerCapture(e.pointerId)) onMove(r)
      }}
      onPointerUp={(e) => {
        const r = at(e.clientX)
        if (!e.currentTarget.hasPointerCapture(e.pointerId)) return
        e.currentTarget.releasePointerCapture(e.pointerId)
        setDragging(false)
        if (r !== null) {
          onMove(r)
          post({ area: 'sides', type: 'split', ratio: r })
        }
      }}
    >
      <div className="absolute inset-y-0 left-[2px] w-px bg-border group-hover:bg-primary group-active:bg-primary" />
    </div>
  )
}

export function Sides() {
  const { sides, focused, ratio } = useArea('sides')
  const main = useRef<HTMLElement>(null)
  // While the divider moves, the frame lays out the sides alone; Rust learns
  // the share once the button is released, and its answer replaces this one.
  const [dragged, setDragged] = useState<number | null>(null)
  useEffect(() => setDragged(null), [ratio])
  const share = dragged ?? ratio
  const find = useArea('find')
  return (
    <main ref={main} className="flex min-w-0 flex-1">
      {sides.map((s, i) => (
        <div key={s.id} className="contents">
          {i > 0 && <Divider main={main} onMove={setDragged} />}
          <Side
            side={s}
            focused={i === focused}
            finding={find.open && find.side === s.id}
            grow={sides.length === 2 ? (i === 0 ? share : 1 - share) : 1}
          />
        </div>
      ))}
    </main>
  )
}
