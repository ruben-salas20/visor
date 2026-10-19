// The window's interface, design D of spikes/maquetas-marco/: the panel
// (area A), the sides (B, with the address rows of C, and the find bar of D
// over the focused side's page) and the status bar (E).
import { Panel } from './areas/files/Panel'
import { StatusBar } from './areas/settings/StatusBar'
import { Sides } from './areas/sides/Sides'

export function App() {
  return (
    <div className="flex h-full flex-col text-[12px]">
      <div className="flex min-h-0 flex-1">
        <Panel />
        <Sides />
      </div>
      <StatusBar />
    </div>
  )
}
