import { createRoot } from 'react-dom/client'
import { TooltipProvider } from '@/components/ui/tooltip'
import './index.css'
import { type Estado } from './comunes'
import { ACTUAL_TEMA, CLARO, aplicarPaleta } from './datos'
import VarianteA from './variantes/A'
import VarianteB from './variantes/B'
import VarianteC from './variantes/C'

// ?v=a|b|c&tema=actual|claro&estado=normal|ajustes|buscar
const q = new URLSearchParams(location.search)
const v = q.get('v') ?? 'a'
const estado = (q.get('estado') ?? 'normal') as Estado
aplicarPaleta(q.get('tema') === 'claro' ? CLARO : ACTUAL_TEMA)

const Variante = v === 'c' ? VarianteC : v === 'b' ? VarianteB : VarianteA

createRoot(document.getElementById('root')!).render(
  <TooltipProvider>
    <Variante estado={estado} />
  </TooltipProvider>,
)
