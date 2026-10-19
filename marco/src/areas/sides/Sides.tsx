// Area B, the sides: each one's tab row, its address row (area C) and the
// hole where Rust puts its pages. Still to come (phase 1): the divider, the
// drop zones, dragging tabs and page fullscreen.
import { Plus, X } from 'lucide-react'
import { AddressRow } from '@/areas/bar/AddressRow'
import { KindIcon } from '@/components/kind-icon'
import { useHole } from '@/holes'
import { cn } from '@/lib/utils'
import { post } from '@/protocol'
import { useArea } from '@/store'
import type { Side as SideState } from './protocol'

const icon = 'grid size-[22px] place-items-center rounded-[3px] text-muted-foreground hover:bg-accent hover:text-foreground'

function TabRow({ side, focused }: { side: SideState; focused: boolean }) {
  return (
    <div className="flex h-[30px] shrink-0 items-stretch bg-sidebar">
      {side.tabs.map((t, i) => {
        const active = i === side.active
        return (
          <div
            key={t.id}
            title={t.target || 'Nueva pestaña'}
            onClick={() => post({ area: 'sides', type: 'select', tab: t.id })}
            onAuxClick={(e) => e.button === 1 && post({ area: 'sides', type: 'close', tab: t.id })}
            className={cn(
              'group relative flex w-[178px] min-w-0 items-center gap-1.5 border-r pr-2 pl-3',
              active ? 'bg-background text-foreground' : 'border-b text-muted-foreground',
              active && focused && 'before:absolute before:inset-x-0 before:top-0 before:h-0.5 before:bg-primary',
            )}
          >
            <KindIcon kind={t.kind} className={cn('size-3', active && focused ? 'text-primary' : 'opacity-70')} />
            <span className={cn('truncate', !focused && 'opacity-80')}>{t.label}</span>
            <button
              title="Cerrar pestaña (Ctrl+W)"
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
      <div className="flex flex-1 items-center border-b pl-1">
        <button className={icon} title="Nueva pestaña (Ctrl+T)" onClick={() => post({ area: 'sides', type: 'new', side: side.id })}>
          <Plus className="size-3.5" />
        </button>
      </div>
    </div>
  )
}

function Side({ side, focused }: { side: SideState; focused: boolean }) {
  const hole = useHole(side.id)
  const focus = () => !focused && post({ area: 'sides', type: 'focus', side: side.id })
  return (
    <section className="flex min-w-0 flex-1 flex-col bg-background" onMouseDown={focus}>
      <TabRow side={side} focused={focused} />
      <AddressRow side={side.id} focused={focused} />
      {/* Rust lays this side's page views over this element. */}
      <div ref={hole} className="min-h-0 flex-1" />
    </section>
  )
}

export function Sides() {
  const { sides, focused } = useArea('sides')
  return (
    <main className="flex min-w-0 flex-1 divide-x divide-border">
      {sides.map((s, i) => (
        <Side key={s.id} side={s} focused={i === focused} />
      ))}
    </main>
  )
}
