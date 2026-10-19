// Area E, the status bar. The settings and the shortcuts list load when they
// open (React.lazy), so they cost nothing until then. Rust says which one is
// open; the frame only reports the clicks and the closes.
import { tr } from '@/i18n'
import { Keyboard, Settings as SettingsIcon } from 'lucide-react'
import { lazy, Suspense } from 'react'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { cn } from '@/lib/utils'
import { post } from '@/protocol'
import { useArea } from '@/store'
import type { TabKind } from '../sides/protocol'
import type { Panel } from './protocol'

const Settings = lazy(() => import('./Settings'))
const Shortcuts = lazy(() => import('./Shortcuts'))

// Each label is a function: `tr` reads the status, which does not exist yet when this module loads.
const KIND: Record<TabKind, () => string> = {
  empty: () => '',
  note: () => 'Markdown',
  drawing: () => tr('Dibujo', 'Drawing'),
  html: () => 'HTML',
  pdf: () => 'PDF',
  image: () => tr('Imagen', 'Image'),
  web: () => 'Web',
  other: () => '',
}

function setPanel(panel: Panel, open: boolean) {
  post(open ? { area: 'settings', type: 'open', panel } : { area: 'settings', type: 'closed', panel })
}

export function StatusBar() {
  const status = useArea('status')
  const item = 'flex h-full items-center gap-1 px-2 hover:bg-accent'
  const kindName = KIND[status.kind]()
  return (
    <footer className="flex h-6 shrink-0 items-center border-t bg-sidebar text-[11px] text-muted-foreground">
      <Popover open={status.settings_open} onOpenChange={(open) => setPanel('settings', open)}>
        <PopoverTrigger className={cn(item, 'pl-3')} title={tr('Ajustes (Ctrl+,)', 'Settings (Ctrl+,)')}>
          <SettingsIcon className="size-3.5" /> {tr('Ajustes', 'Settings')}
        </PopoverTrigger>
        <PopoverContent side="top" align="start" sideOffset={6} className="w-80 p-4">
          <Suspense fallback={null}>
            <Settings status={status} />
          </Suspense>
        </PopoverContent>
      </Popover>
      {status.vault && <span className={item}>{status.vault}</span>}
      <span className="ml-auto" />
      {status.update && <span className={item}>{status.update}</span>}
      {kindName && <span className={item}>{kindName}</span>}
      <span className={item}>Zoom {Math.round(status.zoom * 100)} %</span>
      <button className={item} title={tr('Atajos de teclado (Ctrl+K)', 'Keyboard shortcuts (Ctrl+K)')} onClick={() => setPanel('shortcuts', true)}>
        <Keyboard className="size-3.5" />
      </button>
      <span className={cn(item, 'pr-3')}>v{status.version}</span>
      {status.shortcuts_open && (
        <Suspense fallback={null}>
          <Shortcuts />
        </Suspense>
      )}
    </footer>
  )
}
