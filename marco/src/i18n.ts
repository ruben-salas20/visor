// The frame's texts in the visor's language. Rust sends `lang` in every status
// and it changes only on restart, so the copy in the store is always right.
import { store } from './store'

/** The text in the language in use: Spanish (`es`) or English (`en`). */
export function tr(es: string, en: string): string {
  return store().status.lang === 'en' ? en : es
}
