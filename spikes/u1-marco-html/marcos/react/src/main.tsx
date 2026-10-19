import { createRoot } from 'react-dom/client'
import { flushSync } from 'react-dom'
import './marco'
import './index.css'
import App from './App'

// Mounted synchronously: Rust may call window.marco.pintar() as soon as it hears "listo".
flushSync(() => {
  createRoot(document.getElementById('root')!).render(<App />)
})
