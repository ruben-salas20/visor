# 0038. Nada reemplazado ni con rol hoja dentro del editor

**Estado:** Aceptada · **Fecha:** 2026-10-18

## Contexto

El issue #41: con el editor abierto y un cliente AT-SPI escuchando (un lector de pantalla, cua-driver), Ctrl+E aborta el proceso web con SIGABRT. No es un fallo de visor: es el [bug 279313 de WebKit](https://bugs.webkit.org/show_bug.cgi?id=279313), presente en WebKitGTK 2.54.1.

`boundsForSelection()` cuenta los elementos reemplazados del rango con `characterCount(EmitsObjectReplacementCharacters)` (un U+FFFC por cada uno) y `text()` los omite. `selectionChanged()` traduce esa posición con `UTF16OffsetToUTF8(mapping, bounds.y())` y el índice queda fuera del `mapping` (un `RELEASE_ASSERT` de `WTF::Vector::at`), así que aborta.

Aislado con páginas sueltas (`/tmp` de la sesión, fuera del repo). Hacen falta las cuatro cosas: `contenteditable`, un elemento reemplazado en el rango accesible, un carácter mayor que U+00FF y el cursor al final del texto accesible. Caen 3/3 un `<img>` y un `<input type="checkbox" />`; no caen un `<svg>`, un `div` con `background-image` ni un `<img>` fuera del contenteditable. Además caen, aunque no sean reemplazados, un `div` con `role="img"` y un `<math>` (el MathML de KaTeX): un rol hoja o las matemáticas también cuentan como un carácter que el texto accesible omite.

## Decisión

Ningún widget del editor dibuja un elemento reemplazado ni un elemento cuyo rol WebKit trate como hoja:

- La casilla de tarea es un `<span role="checkbox" aria-checked>`; el dibujo lo pone `note.css`, no un `<input>`.
- La imagen es un `div.md-image` con `background-image` y `role="figure"` (no `role="img"`, que dispara el fallo). El `alt` va en `aria-label` y, si la imagen no carga, como texto; el tamaño lo da la proporción natural del archivo, que el widget lee con un `Image()` suelto.
- Las fórmulas: `#editor .katex-mathml { display: none }` deja el `<math>` fuera del árbol de render solo en el editor; la lectura conserva el MathML.
- El buffer de widget de CodeMirror, un `<img>` que la librería crea por cada widget, pasa a `<span style="display: inline-block">` al construir el paquete (`tools/codemirror/build.mjs`, ver ADR 0028).

Descartado apagar la accesibilidad: `WEBKIT_A11Y_BUS_ADDRESS` a una dirección inexistente lo evita, pero deja sin accesibilidad toda la app (rompe lectores de pantalla y cua-driver). Tampoco lo evitan un `alt` no vacío, `aria-hidden` o `role="presentation"` en el contenteditable, `NO_AT_BRIDGE=1` ni `GTK_A11Y=none`.

## Cifras

2026-10-18, WebKitGTK 2.54.1, `bal` enchufado, sway sin pantalla. Caídas con Ctrl+E y Ctrl+End (con un `visor eval` de por medio, que fuerza la actualización de render), 3–4 repeticiones:

| Nota | Antes | Después |
|---|---|---|
| `lg3.md` (texto) | 2/3 | 0/6 |
| `task.md` (casillas) | 2/3 | 0/4 |
| `img.md` (imagen) | 4/4 | 0/4 |
| `math.md` (KaTeX) | 2/3 | 0/3 |
| `img_broken.md`, `img_inline.md`, `mermaid.md`, `tabla.md` | 0 | 0 |

La página mínima de control sigue cayendo 3/3, así que el arreglo no es apagar la accesibilidad.

Arranque en frío, memoria (PSS), CPU en reposo y fps quedan dentro del ruido, con dos rondas alternadas el mismo día: los rangos de antes y de después se solapan, y la deriva entre rondas (hasta ~35 MB en la misma página con el mismo binario) supera cualquier diferencia.

Paquete de CodeMirror con brotli: 89 406 → 89 382 bytes. Binario: 5 209 632 → 5 210 336 bytes (+704).
