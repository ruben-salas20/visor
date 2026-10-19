// Area E, the status bar. The settings and the shortcuts list load when they
// open (React.lazy), so they cost nothing until then. Still to come (phase 1):
// opening them, and the zoom and update controls.
import { Keyboard, Settings as SettingsIcon } from 'lucide-react'
import { lazy, Suspense, useState } from 'react'
import { cn } from '@/lib/utils'
import { useArea } from '@/store'
import type { TabKind } from '../sides/protocol'

const Settings = lazy(() => import('./Settings'))
const Shortcuts = lazy(() => import('./Shortcuts'))

const KIND: Record<TabKind, string> = {
  empty: '',
  note: 'Markdown',
  drawing: 'Dibujo',
  html: 'HTML',
  pdf: 'PDF',
  image: 'Imagen',
  web: 'Web',
  other: '',
}

export function StatusBar() {
  const status = useArea('status')
  // Phase 1 opens them from the buttons and from Ctrl+, and Ctrl+K.
  const [settings] = useState(false)
  const [shortcuts] = useState(false)
  const item = 'flex h-full items-center gap-1 px-2 hover:bg-accent'
  return (
    <footer className="flex h-6 shrink-0 items-center border-t bg-sidebar text-[11px] text-muted-foreground">
      <button className={cn(item, 'pl-3')} title="Ajustes (Ctrl+,)">
        <SettingsIcon className="size-3.5" /> Ajustes
      </button>
      {status.vault && <span className={item}>{status.vault}</span>}
      <span className="ml-auto" />
      {status.update && <span className={item}>{status.update}</span>}
      {KIND[status.kind] && <span className={item}>{KIND[status.kind]}</span>}
      <span className={item}>Zoom {Math.round(status.zoom * 100)} %</span>
      <button className={item} title="Atajos de teclado (Ctrl+K)">
        <Keyboard className="size-3.5" />
      </button>
      <span className={cn(item, 'pr-3')}>v{status.version}</span>
      <Suspense>
        {settings && <Settings />}
        {shortcuts && <Shortcuts />}
      </Suspense>
    </footer>
  )
}
