// Area E: the list of keyboard shortcuts (Ctrl+K). Loaded when it opens.
// Each row is [keys, what it does in Spanish, in English]. The keys are spelled as `src/agent/help.txt`
// spells them: the Rust test `every_shortcut_is_named_in_help` checks that, reading this file.
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { tr } from '@/i18n'
import { post } from '@/protocol'

export const SHORTCUTS: [string, string, string][] = [
  ['Ctrl+O', 'abrir carpeta', 'open folder'],
  ['Ctrl+B', 'mostrar u ocultar el panel', 'show or hide the panel'],
  ['Ctrl+R / F5', 'recargar', 'reload'],
  ['Ctrl+L', 'barra de direcciones: URL, dominio, ruta de la bóveda, ruta absoluta o búsqueda', 'address bar: URL, domain, vault path, absolute path or search'],
  ['Ctrl+E', 'editar la nota o el dibujo abiertos, o volver a la vista', 'edit the open note or drawing, or go back to the view'],
  ['Ctrl+F', 'buscar en la página', 'search in the page'],
  ['Ctrl+N', 'nueva nota en la carpeta de la actual', 'new note in the folder of the current one'],
  ['Ctrl+Shift+A', 'anotar un elemento o un punto', 'annotate an element or a point'],
  ['Ctrl+Shift+L', 'lista de anotaciones de la página', "list of the page's annotations"],
  ['Ctrl+T', 'pestaña nueva, con el cursor en la barra', 'new tab, with the cursor in the bar'],
  ['Ctrl+W', 'cerrar la pestaña', 'close the tab'],
  ['Ctrl+Q', 'cerrar todas las ventanas; se vuelven a abrir la próxima vez', 'close all windows; they reopen next time'],
  ['Ctrl+Tab', 'pestaña siguiente', 'next tab'],
  ['Ctrl+Shift+Tab', 'pestaña anterior', 'previous tab'],
  ['Ctrl+1 … Ctrl+8', 'pestaña número N del lado con foco', 'tab number N of the focused side'],
  ['Ctrl+9', 'la última pestaña del lado con foco', 'the last tab of the focused side'],
  ['Ctrl+| (o Ctrl+\\)', 'dividir la ventana o volver a un lado', 'split the window or go back to one side'],
  ['Ctrl+Alt+←', 'lado izquierdo', 'left side'],
  ['Ctrl+Alt+→', 'lado derecho', 'right side'],
  ['Ctrl++', 'aumentar el tamaño del texto', 'increase the text size'],
  ['Ctrl+-', 'reducir el tamaño del texto', 'reduce the text size'],
  ['Ctrl+0', 'restablecer el tamaño del texto', 'reset the text size'],
  ['Ctrl+,', 'ajustes: tema, idioma, zoom, buscador y actualizaciones', 'settings: theme, language, zoom, search engine and updates'],
  ['Alt+←', 'atrás', 'back'],
  ['Alt+→', 'adelante', 'forward'],
  ['Ctrl+K', 'esta lista de atajos', 'this list of shortcuts'],
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
          <DialogTitle>{tr('Atajos de teclado', 'Keyboard shortcuts')}</DialogTitle>
        </DialogHeader>
        <div className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-1.5 text-[13px]">
          {SHORTCUTS.map(([keys, es, en]) => (
            <div key={keys} className="contents">
              <span className="font-mono text-xs whitespace-nowrap">{keys}</span>
              <span>{tr(es, en)}</span>
            </div>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  )
}
