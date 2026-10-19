// Area E: the settings panel, in the status bar's popover. Loaded when the
// popover opens. Each choice goes to Rust, which saves it and sends the status back.
import type { ReactNode } from 'react'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { tr } from '@/i18n'
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
        <span className="text-sm font-semibold">{tr('Ajustes', 'Settings')}</span>
        <span className="text-xs text-muted-foreground">Ctrl+,</span>
      </div>
      <Fila titulo={tr('Tema', 'Theme')}>
        <ToggleGroup
          type="single"
          value={prefs.theme ?? SISTEMA}
          onValueChange={(v) => {
            if (v) post({ area: 'settings', type: 'set_theme', choice: pick(v) })
          }}
        >
          <ToggleGroupItem value="sistema">{tr('Sistema', 'System')}</ToggleGroupItem>
          <ToggleGroupItem value="claro">{tr('Claro', 'Light')}</ToggleGroupItem>
          <ToggleGroupItem value="oscuro">{tr('Oscuro', 'Dark')}</ToggleGroupItem>
        </ToggleGroup>
      </Fila>
      <Fila titulo={tr('Idioma', 'Language')}>
        <ToggleGroup
          type="single"
          value={prefs.language ?? SISTEMA}
          onValueChange={(v) => {
            if (v) post({ area: 'settings', type: 'set_language', choice: pick(v) })
          }}
        >
          <ToggleGroupItem value="sistema">{tr('Sistema', 'System')}</ToggleGroupItem>
          <ToggleGroupItem value="es">Español</ToggleGroupItem>
          <ToggleGroupItem value="en">English</ToggleGroupItem>
        </ToggleGroup>
        {status.language_pending && (
          <span className="text-xs leading-snug text-muted-foreground">
            {tr('El idioma cambia al reiniciar visor.', 'The language changes when visor restarts.')}
          </span>
        )}
      </Fila>
      <Fila titulo={tr('Zoom de las páginas', 'Page zoom')}>
        <div className="flex items-center rounded-md border">
          <button className="h-7 w-9 text-base text-muted-foreground hover:text-foreground" title={tr('Reducir (Ctrl+-)', 'Zoom out (Ctrl+-)')} onClick={() => zoom(-1)}>
            −
          </button>
          <button className="h-7 flex-1 border-x text-xs tabular-nums" title={tr('Restablecer (Ctrl+0)', 'Reset (Ctrl+0)')} onClick={() => zoom(0)}>
            {Math.round(status.zoom * 100)} %
          </button>
          <button className="h-7 w-9 text-base text-muted-foreground hover:text-foreground" title={tr('Aumentar (Ctrl++)', 'Zoom in (Ctrl++)')} onClick={() => zoom(1)}>
            +
          </button>
        </div>
      </Fila>
      <Fila titulo={tr('Buscador', 'Search engine')}>
        <Select
          value={prefs.search_engine}
          onValueChange={(id) => {
            if (id) post({ area: 'settings', type: 'set_search_engine', id })
          }}
        >
          <SelectTrigger aria-label={tr('Buscador', 'Search engine')}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {prefs.engines.map((e) => (
              <SelectItem key={e.id} value={e.id}>
                {e.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <span className="text-xs leading-snug text-muted-foreground">
          {tr(
            'Lo que escribas en la barra y no sea una dirección se busca aquí.',
            'What you type in the bar that is not an address is searched here.',
          )}
        </span>
      </Fila>
      <div className="flex flex-col gap-2.5 border-t pt-3">
        <div className="flex items-center justify-between gap-3 text-[13px]">
          {tr('Buscar e instalar actualizaciones', 'Check for and install updates')}
          <Switch checked={prefs.auto_update} onCheckedChange={(on) => post({ area: 'settings', type: 'set_auto_update', on })} />
        </div>
        <div className="flex items-center justify-between gap-3 text-[13px]">
          {tr('Mostrar archivos que visor no abre', 'Show files visor does not open')}
          <Switch checked={prefs.show_other} onCheckedChange={(on) => post({ area: 'settings', type: 'set_show_other', on })} />
        </div>
      </div>
      <div className="flex items-center justify-between gap-2 border-t pt-3 text-xs text-muted-foreground">
        <span className="min-w-0 truncate">{status.update ?? ''}</span>
        {status.update_restart ? (
          <button className="shrink-0 rounded px-1.5 py-0.5 text-foreground hover:bg-accent" onClick={() => post({ area: 'settings', type: 'restart' })}>
            {tr('Reiniciar visor', 'Restart visor')}
          </button>
        ) : (
          <button
            className="shrink-0 rounded px-1.5 py-0.5 text-foreground hover:bg-accent disabled:opacity-50"
            disabled={status.update_busy}
            onClick={() => post({ area: 'settings', type: 'check_updates' })}
          >
            {status.update_busy ? tr('Buscando…', 'Checking…') : tr('Buscar actualización ahora', 'Check for updates now')}
          </button>
        )}
      </div>
    </div>
  )
}
