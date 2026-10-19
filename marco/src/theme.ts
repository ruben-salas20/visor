import type { Colors } from './protocol'

/** Sets the five theme colors; index.css mixes every other one from them. */
export function applyColors(c: Colors) {
  const root = document.documentElement
  root.classList.toggle('dark', c.dark)
  root.style.setProperty('--t-bg', c.bg)
  root.style.setProperty('--t-fg', c.fg)
  root.style.setProperty('--t-accent', c.accent)
  root.style.setProperty('--t-muted', c.muted)
  root.style.setProperty('--t-selection', c.selection)
}
