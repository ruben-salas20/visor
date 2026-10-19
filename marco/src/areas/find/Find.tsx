// Area D: the find bar (Ctrl+F). It lies over the focused side's page, and
// Sides loads it only while it is open. The counter and the search are Rust's.
import { ChevronDown, ChevronUp, X } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { cn } from '@/lib/utils'
import { useOverlay } from '@/overlay'
import { post } from '@/protocol'
import { useArea } from '@/store'

const step = 'grid aspect-square place-items-center rounded-md hover:bg-accent disabled:opacity-40 disabled:hover:bg-transparent'
const check = 'flex items-center gap-1.5 whitespace-nowrap text-xs text-muted-foreground'

export default function Find() {
  const { text: fromRust, match_case, whole_word, label, found, edit } = useArea('find')
  const [text, setText] = useState(fromRust)
  const [matchCase, setMatchCase] = useState(match_case)
  const [wholeWord, setWholeWord] = useState(whole_word)
  const box = useRef<HTMLDivElement>(null)
  const field = useRef<HTMLInputElement>(null)
  useOverlay(box, true)

  // Rust counts each time the bar takes the keyboard (Ctrl+F): the field
  // takes it back then, and WebKit's own focus hand-off after the view gets
  // focus is undone the same way, as in the address row.
  useEffect(() => {
    field.current?.focus()
    field.current?.select()
  }, [edit])
  useEffect(() => {
    const again = () => field.current?.focus()
    window.addEventListener('focus', again)
    return () => window.removeEventListener('focus', again)
  }, [])

  // Rust's copy wins when it changes without the field, such as the bar's
  // reopening on another side. It is set while rendering, as in the address row.
  const [seenEdit, setSeenEdit] = useState(edit)
  if (edit !== seenEdit) {
    setSeenEdit(edit)
    setText(fromRust)
    setMatchCase(match_case)
    setWholeWord(whole_word)
  }

  const search = (next: { text?: string; match_case?: boolean; whole_word?: boolean }) => {
    const t = next.text ?? text
    const m = next.match_case ?? matchCase
    const w = next.whole_word ?? wholeWord
    post({ area: 'find', type: 'search', text: t, match_case: m, whole_word: w })
  }
  const close = () => post({ area: 'find', type: 'close' })

  return (
    <div
      ref={box}
      className="absolute top-0 right-4 z-10 flex items-center gap-2 rounded-b-md border border-t-0 bg-popover px-2 py-1 shadow-lg"
    >
      <div className="flex h-6 w-64 items-center rounded-md border bg-background px-2 ring-1 ring-ring">
        <input
          ref={field}
          autoFocus
          spellCheck={false}
          aria-label="Texto a buscar"
          placeholder="Buscar en la página"
          value={text}
          onChange={(e) => {
            setText(e.target.value)
            search({ text: e.target.value })
          }}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault()
              if (found) post({ area: 'find', type: 'step', forward: !e.shiftKey })
            } else if (e.key === 'Escape') {
              e.preventDefault()
              close()
            }
          }}
          className="min-w-0 flex-1 bg-transparent outline-none"
        />
        <span className="text-xs text-muted-foreground tabular-nums whitespace-nowrap">{label}</span>
      </div>
      <button
        className={cn(step, 'h-6')}
        title="Anterior (Mayús+Enter)"
        disabled={!found}
        onClick={() => post({ area: 'find', type: 'step', forward: false })}
      >
        <ChevronUp className="size-4" />
      </button>
      <button
        className={cn(step, 'h-6')}
        title="Siguiente (Enter)"
        disabled={!found}
        onClick={() => post({ area: 'find', type: 'step', forward: true })}
      >
        <ChevronDown className="size-4" />
      </button>
      <label className={cn(check, 'ml-1')}>
        <input
          type="checkbox"
          className="accent-primary"
          checked={matchCase}
          onChange={(e) => {
            setMatchCase(e.target.checked)
            search({ match_case: e.target.checked })
          }}
        />
        Mayúsculas
      </label>
      <label className={check}>
        <input
          type="checkbox"
          className="accent-primary"
          checked={wholeWord}
          onChange={(e) => {
            setWholeWord(e.target.checked)
            search({ whole_word: e.target.checked })
          }}
        />
        Palabra completa
      </label>
      <button className={cn(step, 'h-6 text-muted-foreground')} title="Cerrar (Esc)" onClick={close}>
        <X className="size-4" />
      </button>
    </div>
  )
}
