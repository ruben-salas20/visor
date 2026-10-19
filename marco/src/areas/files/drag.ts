// A file or folder dragged from the tree, with the pointer and not with the
// system's drag and drop: WebKitGTK's drags never came back to the frame, so
// the tree could not take them. The frame keeps the pointer while the button
// is held, even over the pages laid on top of it, so it also says which side's
// pages are under a dragged file (ADR 0014); Rust marks them and opens the
// file there. The pages never see the drag. The tree listens to move what is
// dropped on it.
import type { PointerEvent } from 'react'
import { holeAt } from '@/holes'
import { post } from '@/protocol'
import { store } from '@/store'

export type Drag = { path: string; folder: boolean; x: number; y: number }

/** Called on every move with the drag, once more with `dropped` when the
 *  button is released, and with null when the drag ends either way. */
export type DragListener = (drag: Drag | null, dropped: boolean) => void

const listeners = new Set<DragListener>()
let pressed: { path: string; folder: boolean; x: number; y: number } | null = null
let current: Drag | null = null
/** The side and zone last reported under a dragged file. */
let zone: { side: number; zone: number } | null = null

/** What the tree is dragging, if anything. */
export function dragged() {
  return current
}

export function listenDrag(f: DragListener) {
  listeners.add(f)
  return () => {
    listeners.delete(f)
  }
}

function emit(drag: Drag | null, dropped: boolean) {
  for (const f of listeners) f(drag, dropped)
}

/** A press on a row that may become a drag once the pointer moves. */
export function pressRow(e: PointerEvent, path: string, folder: boolean) {
  if (e.button !== 0) return
  pressed = { path, folder, x: e.clientX, y: e.clientY }
  window.addEventListener('pointermove', move)
  window.addEventListener('pointerup', up)
  window.addEventListener('pointercancel', cancel)
  window.addEventListener('keydown', escape, true)
}

function move(e: globalThis.PointerEvent) {
  if (!pressed) return
  if (!current) {
    // A few pixels of wobble are still a click.
    if (Math.hypot(e.clientX - pressed.x, e.clientY - pressed.y) < 5) return
    // Rust only hears of files: a folder can only move inside the tree.
    if (!pressed.folder) post({ area: 'files', type: 'drag_start', path: pressed.path })
    document.documentElement.classList.add('cursor-grabbing', 'select-none')
  }
  current = { path: pressed.path, folder: pressed.folder, x: e.clientX, y: e.clientY }
  if (!current.folder) hover(zoneAt(e.clientX, e.clientY))
  emit(current, false)
}

/** The side's zone under a point: 0 left half, 1 right half, 2 the whole
 *  side when the window is split, as the GTK drop zones say it. */
function zoneAt(x: number, y: number) {
  const hole = holeAt(x, y)
  if (!hole) return null
  const whole = store().sides.sides.length === 2
  return { side: hole.side, zone: whole ? 2 : x > hole.rect.left + hole.rect.width / 2 ? 1 : 0 }
}

function hover(next: { side: number; zone: number } | null) {
  if (next?.side === zone?.side && next?.zone === zone?.zone) return
  if (zone && zone.side !== next?.side) post({ area: 'files', type: 'drop_hover', side: zone.side, zone: null })
  if (next) post({ area: 'files', type: 'drop_hover', side: next.side, zone: next.zone })
  zone = next
}

function up(e: globalThis.PointerEvent) {
  if (current) {
    current = { ...current, x: e.clientX, y: e.clientY }
    const on = current.folder ? null : zoneAt(e.clientX, e.clientY)
    if (on) {
      zone = null
      post({ area: 'files', type: 'drop_on', side: on.side, half: on.zone === 2 ? null : on.zone })
    }
    emit(current, true)
    // The release also makes a click on the row under it, which must not open it.
    const swallow = (c: MouseEvent) => c.stopPropagation()
    window.addEventListener('click', swallow, true)
    setTimeout(() => window.removeEventListener('click', swallow, true), 0)
  }
  finish()
}

function cancel() {
  finish()
}

function escape(e: KeyboardEvent) {
  if (e.key !== 'Escape' || !current) return
  e.preventDefault()
  e.stopPropagation()
  finish()
}

function finish() {
  hover(null)
  if (current && !current.folder) post({ area: 'files', type: 'drag_end' })
  const was = current
  current = null
  pressed = null
  window.removeEventListener('pointermove', move)
  window.removeEventListener('pointerup', up)
  window.removeEventListener('pointercancel', cancel)
  window.removeEventListener('keydown', escape, true)
  document.documentElement.classList.remove('cursor-grabbing', 'select-none')
  if (was) emit(null, false)
}
