// Turns dist/ into one self-contained dist/maqueta.html (CSS and JS inlined) so it opens from
// file:// in any engine. A classic <script>, not a module: the bundle has no imports.
import { readFileSync, writeFileSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const dist = path.join(path.dirname(fileURLToPath(import.meta.url)), 'dist')
const indice = readFileSync(path.join(dist, 'index.html'), 'utf8')
const css = /href="\.\/(assets\/[^"]+\.css)"/.exec(indice)[1]
const js = /src="\.\/(assets\/[^"]+\.js)"/.exec(indice)[1]
const html = indice
  .replace(/<link rel="stylesheet"[^>]*>/, () => `<style>${readFileSync(path.join(dist, css), 'utf8')}</style>`)
  .replace(/<script type="module"[^>]*><\/script>/, '')
  .replace('</body>', () => `<script>${readFileSync(path.join(dist, js), 'utf8').replaceAll('</script', '<\\/script')}</script></body>`)
writeFileSync(path.join(dist, 'maqueta.html'), html)
console.log(path.join(dist, 'maqueta.html'))
