// What the frame draws over the pages. The frame lies over them, transparent
// in their holes, and only takes the pointer where it draws (src/ui/cover.rs).
//
// How to use it:
// - The floating parts of components/ui (popover, dropdown-menu, context-menu,
//   tooltip, dialog) declare themselves while mounted: nothing to do.
// - Anything else drawn over a page, such as the find bar:
//   `const ref = useRef<HTMLDivElement>(null); useOverlay(ref, open)`.
// - A drag that may cross a page: `setDragging(true)`, then `false` at the end.
// A click on a page takes the focus from the frame: on `blur`, menus, popovers
// and tooltips close (Escape); dialogs and `useOverlay` boxes stay.

import { useCallback, useEffect, type Ref, type RefObject } from 'react'
import { post, type Rect } from './protocol'

/** Each declared element, and whether the frame's blur closes it. */
const boxes = new Map<Element, boolean>()
let dragging = false
let last = ''
let queued = false

function measure() {
  queued = false
  const list: Rect[] = []
  for (const el of boxes.keys()) {
    const r = el.getBoundingClientRect()
    if (r.width > 0 && r.height > 0) list.push({ x: r.left, y: r.top, w: r.width, h: r.height })
  }
  const text = JSON.stringify([list, dragging])
  if (text !== last) {
    last = text
    post({ area: 'frame', type: 'cover', boxes: list, dragging })
  }
}

function remeasure() {
  if (queued) return
  queued = true
  requestAnimationFrame(measure)
}

const resized = new ResizeObserver(remeasure)
// Radix places floating content with a style on its wrapper, which no resize sees.
const moved = new MutationObserver(remeasure)
window.addEventListener('resize', remeasure)

function add(el: Element, closes: boolean) {
  boxes.set(el, closes)
  resized.observe(el)
  const wrapper = el.parentElement
  if (wrapper && wrapper !== document.body) moved.observe(wrapper, { attributes: true, attributeFilter: ['style'] })
  el.addEventListener('animationend', remeasure)
  remeasure()
}

function remove(el: Element) {
  boxes.delete(el)
  resized.unobserve(el)
  el.removeEventListener('animationend', remeasure)
  remeasure()
}

/** A ref that declares its element while it is mounted and passes it on to
 *  `ref`. For components/ui, whose floating content mounts only while open.
 *  `closes`: the frame's blur closes it. */
export function useOverlayRef<T extends Element>(ref: Ref<T> | undefined, closes = true) {
  return useCallback(
    (el: T | null) => {
      const pass = (v: T | null) => {
        if (typeof ref === 'function') ref(v)
        else if (ref) ref.current = v
      }
      pass(el)
      if (!el) return
      add(el, closes)
      return () => {
        pass(null)
        remove(el)
      }
    },
    [ref, closes],
  )
}

/** Declares `ref`'s element while `open`. The frame's blur leaves it open. */
export function useOverlay(ref: RefObject<Element | null>, open: boolean) {
  useEffect(() => {
    const el = ref.current
    if (!open || !el) return
    add(el, false)
    return () => remove(el)
  }, [ref, open])
}

/** While the frame drags something, it takes the whole window. */
export function setDragging(on: boolean) {
  if (dragging === on) return
  dragging = on
  remeasure()
}

// Radix closes its menus, popovers and tooltips on Escape, the one way in from
// outside them.
window.addEventListener('blur', () => {
  if (![...boxes.values()].some(Boolean)) return
  document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
})
