import { FileText, FileType2, Globe, Image } from 'lucide-react'
import type { TabKind } from '@/areas/sides/protocol'
import { cn } from '@/lib/utils'

// As in design D: pages are text, HTML and websites a globe, PDFs their own.
const ICONS = {
  empty: FileText,
  note: FileText,
  drawing: FileText,
  html: Globe,
  web: Globe,
  pdf: FileType2,
  image: Image,
  other: FileText,
} as const

/** The icon of what a tab or a tree row shows. */
export function KindIcon({ kind, className }: { kind: TabKind; className?: string }) {
  const Icon = ICONS[kind]
  return <Icon className={cn('size-3.5 shrink-0', className)} />
}

/** The kind of a vault path, as Rust's `kind_of` decides it for tabs. */
export function kindOfPath(path: string): TabKind {
  const name = path.toLowerCase()
  if (name.endsWith('.excalidraw.md') || name.endsWith('.excalidraw')) return 'drawing'
  if (name.endsWith('.md')) return 'note'
  if (name.endsWith('.html') || name.endsWith('.htm')) return 'html'
  if (name.endsWith('.pdf')) return 'pdf'
  if (/\.(png|jpe?g|gif|webp|svg|avif)$/.test(name)) return 'image'
  return 'other'
}
