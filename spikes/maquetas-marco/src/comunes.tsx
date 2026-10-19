import { ChevronDown, ChevronUp, FileText, FileType2, Globe, X } from 'lucide-react'
import type { ReactNode } from 'react'
import { Switch } from '@/components/ui/switch'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { cn } from '@/lib/utils'
import { type Tipo } from './datos'

export type Estado = 'normal' | 'ajustes' | 'buscar'

export function IconoArchivo({ tipo, className }: { tipo: Tipo; className?: string }) {
  const Icono = tipo === 'html' ? Globe : tipo === 'pdf' ? FileType2 : FileText
  return <Icono className={cn('size-3.5 shrink-0', className)} />
}

/** Stand-in for the page webview. In visor this hole is another webview, not frame HTML. */
export function Pagina({ cual, buscar, className }: { cual: 'cambio' | 'dobles'; buscar?: boolean; className?: string }) {
  const j = (n: number) =>
    buscar ? <mark className={n === 3 ? 'actual' : undefined}>jacobiano</mark> : 'jacobiano'
  if (cual === 'dobles')
    return (
      <article className={cn('pagina h-full overflow-hidden', className)}>
        <h1>Integrales dobles</h1>
        <p>
          Una integral doble suma los valores de una función sobre una región del plano. Si la función es positiva, mide
          el volumen bajo su gráfica.
        </p>
        <h2>Sobre un rectángulo</h2>
        <p>
          Se divide el rectángulo <code>R = [a, b] × [c, d]</code> en celdas pequeñas, se toma un punto en cada una y se
          suma <code>f(x, y) · ΔA</code>. El límite de esas sumas es la integral.
        </p>
        <p>
          En la práctica no se calcula así: el teorema de Fubini permite integrar primero en una variable y después en la
          otra.
        </p>
      </article>
    )
  return (
    <article className={cn('pagina h-full overflow-hidden', className)}>
      <h1>Cambio de variables</h1>
      <p>
        Cambiar de variables simplifica una integral doble cuando la región o la función tienen una forma natural en otras
        coordenadas. El factor que corrige las áreas es el valor absoluto del {j(1)}.
      </p>
      <h2>La fórmula</h2>
      <p>
        Si <code>T(u, v) = (x, y)</code> es biyectiva y derivable, entonces{' '}
        <code>∬ f(x, y) dA = ∬ f(T(u, v)) · |J| du dv</code>, donde <code>J</code> es el {j(2)} de <code>T</code>.
      </p>
      <h2>Ejemplo: coordenadas polares</h2>
      <p>
        Con <code>x = r cos θ</code> e <code>y = r sen θ</code>, el {j(3)} vale <code>r</code>. Por eso aparece una{' '}
        <code>r</code> de más al pasar a polares: <code>dA = r dr dθ</code>.
      </p>
      <p>
        Un error frecuente es olvidar ese factor. Otro, no comprobar que la transformación cubre la región una sola vez.
      </p>
    </article>
  )
}

function Fila({ titulo, children }: { titulo: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <span className="text-xs text-muted-foreground">{titulo}</span>
      {children}
    </div>
  )
}

function Interruptor({ texto, activo }: { texto: string; activo: boolean }) {
  return (
    <label className="flex items-center justify-between gap-3 text-[13px]">
      <span>{texto}</span>
      <Switch defaultChecked={activo} />
    </label>
  )
}

/** The settings of src/ui/sidebar.rs `settings_popover`, same order and wording. */
export function Ajustes() {
  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-baseline justify-between">
        <span className="text-sm font-semibold">Ajustes</span>
        <span className="text-xs text-muted-foreground">Ctrl+,</span>
      </div>
      <Fila titulo="Tema">
        <ToggleGroup type="single" defaultValue="sistema">
          <ToggleGroupItem value="sistema">Sistema</ToggleGroupItem>
          <ToggleGroupItem value="claro">Claro</ToggleGroupItem>
          <ToggleGroupItem value="oscuro">Oscuro</ToggleGroupItem>
        </ToggleGroup>
      </Fila>
      <Fila titulo="Idioma">
        <ToggleGroup type="single" defaultValue="sistema">
          <ToggleGroupItem value="sistema">Sistema</ToggleGroupItem>
          <ToggleGroupItem value="es">Español</ToggleGroupItem>
          <ToggleGroupItem value="en">English</ToggleGroupItem>
        </ToggleGroup>
      </Fila>
      <Fila titulo="Zoom de las páginas">
        <div className="flex items-center rounded-md border">
          <button className="h-7 w-9 text-base text-muted-foreground hover:text-foreground" title="Reducir (Ctrl+-)">
            −
          </button>
          <button className="h-7 flex-1 border-x text-xs tabular-nums" title="Restablecer (Ctrl+0)">
            110 %
          </button>
          <button className="h-7 w-9 text-base text-muted-foreground hover:text-foreground" title="Aumentar (Ctrl++)">
            +
          </button>
        </div>
      </Fila>
      <Fila titulo="Buscador">
        <div className="flex h-7 items-center justify-between rounded-md border px-2.5 text-[13px]">
          DuckDuckGo <ChevronDown className="size-3.5 text-muted-foreground" />
        </div>
        <span className="text-xs leading-snug text-muted-foreground">
          Lo que escribas en la barra y no sea una dirección se busca aquí.
        </span>
      </Fila>
      <div className="flex flex-col gap-2.5 border-t pt-3">
        <Interruptor texto="Buscar e instalar actualizaciones" activo />
        <Interruptor texto="Mostrar archivos que visor no abre" activo={false} />
      </div>
      <div className="flex items-center justify-end border-t pt-3 text-xs text-muted-foreground">
        <button className="rounded px-1.5 py-0.5 text-foreground hover:bg-accent">Buscar actualización ahora</button>
      </div>
    </div>
  )
}

/** Ctrl+F bar: the controls of src/ui/panes.rs `FindBar`. */
export function Buscar({ className, compacto }: { className?: string; compacto?: boolean }) {
  const alto = compacto ? 'h-6' : 'h-7'
  return (
    <div className={cn('flex items-center gap-2', className)}>
      <div className={cn('flex w-64 items-center rounded-md border bg-background px-2 ring-1 ring-ring', alto)}>
        <span className="flex-1 truncate">jacobiano</span>
        <span className="text-xs text-muted-foreground tabular-nums">3 de 12</span>
      </div>
      <button className={cn('grid aspect-square place-items-center rounded-md hover:bg-accent', alto)} title="Anterior (Mayús+Enter)">
        <ChevronUp className="size-4" />
      </button>
      <button className={cn('grid aspect-square place-items-center rounded-md hover:bg-accent', alto)} title="Siguiente (Enter)">
        <ChevronDown className="size-4" />
      </button>
      <label className="ml-1 flex items-center gap-1.5 whitespace-nowrap text-xs text-muted-foreground">
        <input type="checkbox" className="accent-[var(--t-accent)]" /> Mayúsculas
      </label>
      <label className="flex items-center gap-1.5 whitespace-nowrap text-xs text-muted-foreground">
        <input type="checkbox" className="accent-[var(--t-accent)]" /> Palabra completa
      </label>
      <button
        className={cn('ml-auto grid aspect-square place-items-center rounded-md text-muted-foreground hover:bg-accent', alto)}
        title="Cerrar (Esc)"
      >
        <X className="size-4" />
      </button>
    </div>
  )
}
