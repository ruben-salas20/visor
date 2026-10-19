// Tells Rust where each side's pages go. The pages are webviews laid over
// the frame; the frame leaves an empty element for them and reports its
// rectangle whenever the layout may have moved it.

import { useCallback } from 'react'
import { post, type Hole } from './protocol'

const holes = new Map<number, HTMLElement>()
let last = ''
let queued = false

function measure() {
  queued = false
  const list: Hole[] = []
  for (const [side, el] of holes) {
    const r = el.getBoundingClientRect()
    list.push({ side, x: r.left, y: r.top, w: r.width, h: r.height })
  }
  const text = JSON.stringify(list)
  if (text !== last) {
    last = text
    post({ area: 'frame', type: 'holes', holes: list })
  }
}

/** Measures on the next frame, once however many times it is asked. */
export function remeasure() {
  if (queued) return
  queued = true
  requestAnimationFrame(measure)
}

const resized = new ResizeObserver(remeasure)
window.addEventListener('resize', remeasure)

/** A ref for the element that holds side `side`'s pages. */
export function useHole(side: number) {
  return useCallback(
    (el: HTMLElement | null) => {
      const old = holes.get(side)
      if (old) resized.unobserve(old)
      if (el) {
        holes.set(side, el)
        resized.observe(el)
      } else {
        holes.delete(side)
      }
      remeasure()
    },
    [side],
  )
}
