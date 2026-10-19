import { useEffect, useRef, useState } from 'react'
import { ArrowLeft, ArrowRight, BookOpen, RotateCw } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Separator } from '@/components/ui/separator'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip'
import { Arbol } from './Arbol'
import { Paneles } from './Paneles'
import { acciones, leer, suscribir } from './marco'

function useMarco() {
  const [estado, setEstado] = useState(leer)
  useEffect(() => {
    const quitar = suscribir(setEstado)
    // A pintar() may have run before this effect subscribed.
    setEstado(leer())
    return quitar
  }, [])
  return estado
}

// Back, forward and reload do nothing in this prototype; they only show the frame's look.
function BotonDecorativo({ icono: Icono, texto }: { icono: typeof ArrowLeft; texto: string }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant="ghost" size="icon-sm" className="text-muted-foreground" aria-label={texto}>
          <Icono />
        </Button>
      </TooltipTrigger>
      <TooltipContent>{texto} (sin función en el prototipo)</TooltipContent>
    </Tooltip>
  )
}

export default function App() {
  const { arbol, paneles, foco } = useMarco()
  const contenedor = useRef<HTMLElement>(null)

  useEffect(() => {
    // Rust needs the rectangles again whenever the window or a panel changes size.
    const vigilar = new ResizeObserver(() => acciones.enviarRectas())
    vigilar.observe(contenedor.current!)
    document.querySelectorAll('.contenido').forEach((el) => vigilar.observe(el))
    acciones.enviarListo()
    return () => vigilar.disconnect()
  }, [])

  return (
    <TooltipProvider>
    <div className="flex h-full flex-col">
      <header className="flex h-11 shrink-0 items-center gap-1 border-b bg-card/80 px-3 backdrop-blur">
        <div className="flex items-center gap-2 pr-2">
          <span className="grid size-6 place-items-center rounded-md bg-primary text-primary-foreground">
            <BookOpen className="size-3.5" />
          </span>
          <span className="text-sm font-semibold tracking-tight">U1 marco React</span>
        </div>
        <Separator orientation="vertical" className="mx-1 !h-5" />
        <BotonDecorativo icono={ArrowLeft} texto="Atrás" />
        <BotonDecorativo icono={ArrowRight} texto="Adelante" />
        <BotonDecorativo icono={RotateCw} texto="Recargar" />
        <span className="ml-auto truncate pl-4 text-xs text-muted-foreground">
          Ctrl+W cierra la pestaña activa del panel con foco
        </span>
      </header>

      <div className="flex min-h-0 flex-1">
        <aside className="flex w-[260px] shrink-0 flex-col border-r bg-sidebar">
          <div className="px-4 pt-3 pb-1.5 text-[11px] font-semibold tracking-wider text-muted-foreground uppercase">
            Archivos
          </div>
          <Arbol lista={arbol} />
        </aside>
        <main ref={contenedor} className="flex min-w-0 flex-1 gap-2 p-2">
          <Paneles paneles={paneles} foco={foco} />
        </main>
      </div>
    </div>
    </TooltipProvider>
  )
}
