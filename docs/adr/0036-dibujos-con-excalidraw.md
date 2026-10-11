# 0036. Dibujos con la librería oficial de Excalidraw

**Estado:** Aceptada · **Fecha:** 2026-10-10

## Contexto

Las bóvedas de Obsidian tienen dibujos `.excalidraw.md` (plugin de Excalidraw) y `.excalidraw` (excalidraw.com). La alternativa era pintarlos con rough.js: unos 9,5 KB con brotli, pero solo lectura, y con un dibujante propio que nunca sería igual al original.

## Decisión

La librería oficial (`@excalidraw/excalidraw` 0.18.1), para ver y editar. Igual que las notas (0026): se abren en lectura, Ctrl+E alterna con la edición y se guarda solo a los 800 ms. Excalidraw solo se carga en la página de un dibujo.

- Se guarda en la forma en que estaba el archivo: `compressed-json` o `json`. En el `.md` se regenera `## Text Elements` desde el JSON, porque Obsidian da prioridad a esa sección. El resto del archivo no se toca.
- Fuera del paquete (`tools/excalidraw/build.mjs`):
  - Mermaid y KaTeX: solo sirven para convertir texto Mermaid en formas. El build falla si entran.
  - El recorte de fuentes en WebAssembly (1,8 MB): solo sirve al exportar SVG. Sin él, la fuente se incrusta entera, que es lo mismo que hace la librería si el recorte falla.
  - Los idiomas salvo `en` y `es-ES`, y la fuente CJK Xiaolai (13 MB).
- Sin CDN: `EXCALIDRAW_ASSET_PATH` apunta a los assets de visor y el respaldo de fuentes a esm.sh se reescribe al mismo servidor.
- En un `.excalidraw.md` no se insertan imágenes nuevas: guardarlas exige crear archivos en la bóveda (issue #6).

## Cifras

- Paquete con brotli: JS 360 KB y CSS 19 KB, más 440 KB de woff2 de 8 familias. Con el recorte de fuentes eran 966 KB de JS.
- Binario: 4 435 496 → 5 284 576 bytes (+849 KB).
- `medir.py`, 2 rondas, enchufado (mediana antes/después): arranque mínimo 215/216 ms, memoria en reposo 185,2/186,4 MB y CPU en reposo 0,167/0,133 %. Sin cambios.
- PSS con la página abierta: nota 186 MB en lectura y 207 MB en edición; dibujo 231 MB en lectura y 240 MB en edición. Un dibujo cuesta unos +45 MB frente a una nota, solo mientras está abierto. Abrirlo tarda unos 320 ms.
