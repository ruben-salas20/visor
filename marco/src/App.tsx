// The window's interface, design D of spikes/maquetas-marco/: the panel
// (area A), the sides (B, with the address rows of C) and the status bar (E).
// The find bar (D) loads when it opens.
import { lazy, Suspense, useState } from 'react'
import { Panel } from './areas/files/Panel'
import { StatusBar } from './areas/settings/StatusBar'
import { Sides } from './areas/sides/Sides'

const Find = lazy(() => import('./areas/find/Find'))

export function App() {
  // Phase 1: Ctrl+F opens it on the focused side.
  const [find] = useState(false)
  return (
    <div className="flex h-full flex-col text-[12px]">
      <div className="flex min-h-0 flex-1">
        <Panel />
        <Sides />
      </div>
      <StatusBar />
      <Suspense>{find && <Find />}</Suspense>
    </div>
  )
}
