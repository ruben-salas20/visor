// Area E: the list of keyboard shortcuts (Ctrl+K). Loaded when it opens.
// Each row is [keys, what it does]. The keys are spelled as `src/agent/help.txt`
// spells them: the Rust test `every_shortcut_is_named_in_help` checks that, reading this file.
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { post } from '@/protocol'

export const SHORTCUTS: [string, string][] = [
  ['Ctrl+O', 'abrir carpeta'],
  ['Ctrl+B', 'mostrar u ocultar el panel'],
  ['Ctrl+R / F5', 'recargar'],
  ['Ctrl+L', 'barra de direcciones: URL, dominio, ruta de la bóveda, ruta absoluta o búsqueda'],
  ['Ctrl+Shift+B', 'mostrar u ocultar la barra de direcciones'],
  ['Ctrl+E', 'editar la nota o el dibujo abiertos, o volver a la vista'],
  ['Ctrl+F', 'buscar en la página'],
  ['Ctrl+N', 'nueva nota en la carpeta de la actual'],
  ['Ctrl+Shift+A', 'anotar un elemento o un punto'],
  ['Ctrl+Shift+L', 'lista de anotaciones de la página'],
  ['Ctrl+T', 'pestaña nueva, con el cursor en la barra'],
  ['Ctrl+W', 'cerrar la pestaña'],
  ['Ctrl+Q', 'cerrar todas las ventanas; se vuelven a abrir la próxima vez'],
  ['Ctrl+Tab', 'pestaña siguiente'],
  ['Ctrl+Shift+Tab', 'pestaña anterior'],
  ['Ctrl+1 … Ctrl+8', 'pestaña número N del lado con foco'],
  ['Ctrl+9', 'la última pestaña del lado con foco'],
  ['Ctrl+| (o Ctrl+\\)', 'dividir la ventana o volver a un lado'],
  ['Ctrl+Alt+←', 'lado izquierdo'],
  ['Ctrl+Alt+→', 'lado derecho'],
  ['Ctrl++', 'aumentar el tamaño del texto'],
  ['Ctrl+-', 'reducir el tamaño del texto'],
  ['Ctrl+0', 'restablecer el tamaño del texto'],
  ['Ctrl+,', 'ajustes: tema, idioma, zoom, buscador y actualizaciones'],
  ['Alt+←', 'atrás'],
  ['Alt+→', 'adelante'],
  ['Ctrl+K', 'esta lista de atajos'],
]

function close() {
  post({ area: 'settings', type: 'closed', panel: 'shortcuts' })
}

export default function Shortcuts() {
  return (
    <Dialog open onOpenChange={(open) => !open && close()}>
      <DialogContent
        className="max-w-md"
        onKeyDown={(e) => {
          // The keys go to the frame while this list has the focus; Ctrl+K closes it, as it opened it.
          if (e.ctrlKey && e.key.toLowerCase() === 'k') {
            e.preventDefault()
            close()
          }
        }}
      >
        <DialogHeader>
          <DialogTitle>Atajos de teclado</DialogTitle>
        </DialogHeader>
        <div className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-1.5 text-[13px]">
          {SHORTCUTS.map(([keys, what]) => (
            <div key={keys} className="contents">
              <span className="font-mono text-xs whitespace-nowrap">{keys}</span>
              <span>{what}</span>
            </div>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  )
}
