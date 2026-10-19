// Area E: the list of keyboard shortcuts (Ctrl+K). Loaded when it opens.
// Grouped by what they act on; each row is [keys, what it does in Spanish, in English]. The keys are spelled as `src/agent/help.txt`
// spells them: the Rust test `every_shortcut_is_named_in_help` checks that, reading this file.
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { tr } from '@/i18n'
import { post } from '@/protocol'

type Group = { es: string; en: string; rows: [string, string, string][] }

export const SHORTCUTS: Group[] = [
  {
    es: 'Pestañas',
    en: 'Tabs',
    rows: [
      ['Ctrl+T', 'pestaña nueva, con el cursor en la barra', 'new tab, with the cursor in the bar'],
      ['Ctrl+W', 'cerrar la pestaña', 'close the tab'],
      ['Ctrl+Tab', 'pestaña siguiente', 'next tab'],
      ['Ctrl+Shift+Tab', 'pestaña anterior', 'previous tab'],
      ['Ctrl+1 … Ctrl+8', 'pestaña número N del lado con foco', 'tab number N of the focused side'],
      ['Ctrl+9', 'la última pestaña del lado con foco', 'the last tab of the focused side'],
    ],
  },
  {
    es: 'Ventana y lados',
    en: 'Window and sides',
    rows: [
      ['Ctrl+B', 'mostrar u ocultar el panel', 'show or hide the panel'],
      ['Ctrl+| (o Ctrl+\\)', 'dividir la ventana o volver a un lado', 'split the window or go back to one side'],
      ['Ctrl+Alt+←', 'lado izquierdo', 'left side'],
      ['Ctrl+Alt+→', 'lado derecho', 'right side'],
    ],
  },
  {
    es: 'Página',
    en: 'Page',
    rows: [
      ['Ctrl+L', 'barra de direcciones: URL, dominio, ruta de la bóveda, ruta absoluta o búsqueda', 'address bar: URL, domain, vault path, absolute path or search'],
      ['Alt+←', 'atrás', 'back'],
      ['Alt+→', 'adelante', 'forward'],
      ['Ctrl+R / F5', 'recargar', 'reload'],
      ['Ctrl+F', 'buscar en la página', 'search in the page'],
      ['Ctrl++', 'aumentar el tamaño del texto', 'increase the text size'],
      ['Ctrl+-', 'reducir el tamaño del texto', 'reduce the text size'],
      ['Ctrl+0', 'restablecer el tamaño del texto', 'reset the text size'],
    ],
  },
  {
    es: 'Notas y anotaciones',
    en: 'Notes and annotations',
    rows: [
      ['Ctrl+N', 'nueva nota en la carpeta de la actual', 'new note in the folder of the current one'],
      ['Ctrl+E', 'editar la nota o el dibujo abiertos, o volver a la vista', 'edit the open note or drawing, or go back to the view'],
      ['Ctrl+Shift+A', 'anotar un elemento o un punto', 'annotate an element or a point'],
      ['Ctrl+Shift+L', 'lista de anotaciones de la página', "list of the page's annotations"],
    ],
  },
  {
    es: 'visor',
    en: 'visor',
    rows: [
      ['Ctrl+O', 'abrir carpeta', 'open folder'],
      ['Ctrl+,', 'ajustes: tema, idioma, zoom, buscador y actualizaciones', 'settings: theme, language, zoom, search engine and updates'],
      ['Ctrl+K', 'esta lista de atajos', 'this list of shortcuts'],
      ['Ctrl+Q', 'cerrar todas las ventanas; se vuelven a abrir la próxima vez', 'close all windows; they reopen next time'],
    ],
  },
]

function close() {
  post({ area: 'settings', type: 'closed', panel: 'shortcuts' })
}

export default function Shortcuts() {
  return (
    <Dialog open onOpenChange={(open) => !open && close()}>
      <DialogContent
        className="max-w-lg"
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
        <div className="max-h-[70vh] overflow-auto rounded-md border">
          <table className="w-full border-collapse text-[13px]">
            <thead className="sticky top-0 bg-muted text-left text-xs text-muted-foreground">
              <tr>
                <th className="px-3 py-1.5 font-semibold">{tr('Atajo', 'Shortcut')}</th>
                <th className="px-3 py-1.5 font-semibold">{tr('Qué hace', 'What it does')}</th>
              </tr>
            </thead>
            {SHORTCUTS.map((group) => (
              <tbody key={group.en}>
                <tr className="border-t">
                  <th colSpan={2} className="bg-muted/60 px-3 pt-2.5 pb-1 text-left text-xs font-semibold">
                    {tr(group.es, group.en)}
                  </th>
                </tr>
                {group.rows.map(([keys, es, en]) => (
                  <tr key={keys} className="border-t">
                    <td className="px-3 py-1.5 align-top whitespace-nowrap">
                      <kbd className="rounded border bg-background px-1.5 py-0.5 font-mono text-xs shadow-[0_1px_0_var(--border)]">{keys}</kbd>
                    </td>
                    <td className="px-3 py-1.5">{tr(es, en)}</td>
                  </tr>
                ))}
              </tbody>
            ))}
          </table>
        </div>
      </DialogContent>
    </Dialog>
  )
}
