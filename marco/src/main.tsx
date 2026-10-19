import { createRoot } from 'react-dom/client'
import { TooltipProvider } from '@/components/ui/tooltip'
import { App } from './App'
import './index.css'
import { post } from './protocol'
import './store'

createRoot(document.getElementById('root')!).render(
  <TooltipProvider>
    <App />
  </TooltipProvider>,
)

// Rust answers with every snapshot; `store` already listens.
post({ area: 'frame', type: 'ready' })
