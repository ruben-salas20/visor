// Area E: the settings panel, in the status bar's popover. Loaded when the
// popover opens. Each choice goes to Rust, which saves it and sends the status back.
import type { ReactNode } from 'react'
import { Switch } from '@/components/ui/switch'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { post } from '@/protocol'
import type { Status } from './protocol'

// The system's choice in the toggles. Rust takes `null` for it.
const SISTEMA = 'sistema'

function pick(value: string): string | null {
  return value === SISTEMA ? null : value
}

function Fila({ titulo, children }: { titulo: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <span className="text-xs font-medium">{titulo}</span>
      {children}
    </div>
  )
}

export default function Settings({ status }: { status: Status }) {
  const { prefs } = status
  const zoom = (step: number) => post({ area: 'settings', type: 'zoom', step })
  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-baseline justify-between">
        <span className="text-sm font-semibold">Ajustes</span>
        <span className="text-xs text-muted-foreground">Ctrl+,</span>
      </div>
      <Fila titulo="Tema">
        <ToggleGroup
          type="single"
          value={prefs.theme ?? SISTEMA}
          onValueChange={(v) => {
            if (v) post({ area: 'settings', type: 'set_theme', choice: pick(v) })
          }}
        >
          <ToggleGroupItem value="sistema">Sistema</ToggleGroupItem>
          <ToggleGroupItem value="claro">Claro</ToggleGroupItem>
          <ToggleGroupItem value="oscuro">Oscuro</ToggleGroupItem>
        </ToggleGroup>
      </Fila>
      <Fila titulo="Idioma">
        <ToggleGroup
          type="single"
          value={prefs.language ?? SISTEMA}
          onValueChange={(v) => {
            if (v) post({ area: 'settings', type: 'set_language', choice: pick(v) })
          }}
        >
          <ToggleGroupItem value="sistema">Sistema</ToggleGroupItem>
          <ToggleGroupItem value="es">Español</ToggleGroupItem>
          <ToggleGroupItem value="en">English</ToggleGroupItem>
        </ToggleGroup>
        {status.language_pending && (
          <span className="text-xs leading-snug text-muted-foreground">El idioma cambia al reiniciar visor.</span>
        )}
      </Fila>
      <Fila titulo="Zoom de las páginas">
        <div className="flex items-center rounded-md border">
          <button className="h-7 w-9 text-base text-muted-foreground hover:text-foreground" title="Reducir (Ctrl+-)" onClick={() => zoom(-1)}>
            −
          </button>
          <button className="h-7 flex-1 border-x text-xs tabular-nums" title="Restablecer (Ctrl+0)" onClick={() => zoom(0)}>
            {Math.round(status.zoom * 100)} %
          </button>
          <button className="h-7 w-9 text-base text-muted-foreground hover:text-foreground" title="Aumentar (Ctrl++)" onClick={() => zoom(1)}>
            +
          </button>
        </div>
      </Fila>
      <Fila titulo="Buscador">
        <ToggleGroup
          type="single"
          value={prefs.search_engine}
          onValueChange={(id) => {
            if (id) post({ area: 'settings', type: 'set_search_engine', id })
          }}
        >
          {prefs.engines.map((e) => (
            <ToggleGroupItem key={e.id} value={e.id}>
              {e.label}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
        <span className="text-xs leading-snug text-muted-foreground">Lo que escribas en la barra y no sea una dirección se busca aquí.</span>
      </Fila>
      <div className="flex flex-col gap-2.5 border-t pt-3">
        <div className="flex items-center justify-between gap-3 text-[13px]">
          Buscar e instalar actualizaciones
          <Switch checked={prefs.auto_update} onCheckedChange={(on) => post({ area: 'settings', type: 'set_auto_update', on })} />
        </div>
        <div className="flex items-center justify-between gap-3 text-[13px]">
          Mostrar archivos que visor no abre
          <Switch checked={prefs.show_other} onCheckedChange={(on) => post({ area: 'settings', type: 'set_show_other', on })} />
        </div>
      </div>
      <div className="flex items-center justify-between gap-2 border-t pt-3 text-xs text-muted-foreground">
        <span className="min-w-0 truncate">{status.update ?? ''}</span>
        {status.update_restart ? (
          <button className="shrink-0 rounded px-1.5 py-0.5 text-foreground hover:bg-accent" onClick={() => post({ area: 'settings', type: 'restart' })}>
            Reiniciar visor
          </button>
        ) : (
          <button
            className="shrink-0 rounded px-1.5 py-0.5 text-foreground hover:bg-accent disabled:opacity-50"
            disabled={status.update_busy}
            onClick={() => post({ area: 'settings', type: 'check_updates' })}
          >
            {status.update_busy ? 'Buscando…' : 'Buscar actualización ahora'}
          </button>
        )}
      </div>
    </div>
  )
}
