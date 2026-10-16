# 0037. Visor de PDF con PDF.js

**Estado:** Aceptada · **Fecha:** 2026-10-16

## Contexto

Las bóvedas guardan PDF junto a las notas (libros, ejercicios, apuntes). Desde #37 se abren desde el panel y con `[[archivo.pdf]]`, pero en el visor de PDF.js que trae WebKitGTK: vive en un marco de esquema `webkit-pdfjs-viewer://`, donde los scripts de visor no entran, así que no admitía las anotaciones de visor (#27).

La alternativa de 0 KB, cargar el PDF.js de WebKit desde una página de visor, no se probó: depende de la versión de WebKit del sistema y no existe en Windows (#35), donde WebView2 no trae PDF.js. PDF.js es el visor de Firefox, con licencia Apache-2.0.

## Decisión

El visor completo de PDF.js 6.4.299 (`web/viewer.html`, el de Firefox, el mismo que traía WebKit), servido por visor. Lo empaqueta `tools/pdfjs/build.mjs` en `assets/pdfjs/` a partir de dos fuentes de la misma versión: `pdfjs-dist` (biblioteca y worker) y el zip oficial con su SHA-256 (el visor). Ver `tools/pdfjs/README.md`. Igual que el resto de páginas de visor, se sirve desde la bóveda y nunca desde una CDN.

Primero se probó solo el componente `pdf_viewer.mjs`, con una barra propia (página, búsqueda, esquema). La persona echó de menos las funciones del visor de antes (miniaturas, zoom, girar, presentación, imprimir), y la barra propia tenía sus propios fallos (las páginas no se distinguían del fondo y la barra cambiaba de tamaño con el zoom). El visor completo cuesta unos 79 KB más en el binario.

- Un `.pdf` de la bóveda se abre en una página de visor (`src/vault/pdf.rs`): la cabecera la escribe Rust, y `src/page/pdf.js` mete el cuerpo de `viewer.html` (`viewer-body.html`) y arranca el visor. El PDF se lee del mismo archivo con `?raw` (`http.rs`), que sirve peticiones por rangos.
- Opciones fijadas antes de arrancar (`webviewerloaded`), con las preferencias desactivadas para que nada guardado por el visor las cambie:
  - `enableScripting: false`: el JavaScript incrustado en un PDF no se ejecuta. PDF.js 6.4 ya no compila código a partir de las fuentes (la opción `isEvalSupported` desapareció; CVE-2024-4367 se corrigió en 4.2).
  - `annotationEditorMode: -1`: sin las herramientas de dibujar, resaltar, texto, sello y firma del visor. Chocaban con las anotaciones de visor y, al descargar, cambiarían el PDF.
  - `disableHistory`: la dirección de la página no cambia con `#page=`.
  - Textos del visor en el idioma de visor (`es-ES` o `en-US`).
- Se ocultan «Abrir archivo» (Ctrl+O es de visor) y «Vista actual». Una dirección con `?file=` no abre nada: mostraría otro PDF en la página cuyas anotaciones son las de este.
- Tema: el visor sigue `color-scheme`, que la página toma de visor (`--visor-mode` y el evento `visor:tema`).
- Zoom: en un PDF, Rust deja el zoom de WebKit en 1, y Ctrl+más, Ctrl+menos y Ctrl+0 van al zoom del propio visor (`page_calls::pdf_zoom`). PDF.js dibuja los lienzos al tamaño en que se ven, y la barra no cambia de tamaño. El zoom de visor del panel de ajustes no afecta a los PDF.
- «Abrir con otra aplicación», el primero del menú del visor, usa un mensaje sin ruta (`open_pdf_elsewhere`). Solo lo acepta una página de la bóveda (`trusted`), y Rust abre el PDF que muestra la vista, comprobado con `vault::pdf::file_for`. Se salta visor si es el programa predeterminado para PDF.
- Solo los PDF de la bóveda usan este visor. Un PDF fuera de toda bóveda se abre suelto, como un HTML suelto, en el visor de WebKit y sin anotaciones. Un PDF no es página (`vault::is_page`): no se indexa como nota ni se edita.
- Contrato para las anotaciones: al dibujarse cada capa de texto, la página lanza `visor:pdf-textlayer` con `{ page }`, y expone `window.__visorPdf = { ready, goToPage, zoom }`.
- Funciona igual en Chromium (comprobado sin ventana), así que sirve también para WebView2 en Windows (#35).

## Lo que queda fuera

- **Carpetas de PDF.js no empaquetadas**: `cmaps/`, `standard_fonts/`, `iccs/`, `quickjs*`, `*_nowasm_fallback.js`, `.map`, `legacy/`, `debugger.mjs` y los idiomas que no son `en-US` ni `es-ES`. Sin `cmaps/` los textos en CJK con codificaciones no incrustadas pueden salir mal, y sin `standard_fonts/` las fuentes estándar no incrustadas usan las del sistema. No se ha medido el efecto.
- **Imprimir y descargar dentro de visor**: dependen de que WebKitGTK abra el diálogo de impresión y acepte descargas. Sin comprobar; si no funcionan, quedan para «Abrir con otra aplicación».
- **Tema oscuro**: solo cambian los colores del visor (barra, panel, fondo). Las páginas del PDF conservan su color de papel, como en cualquier lector.
- **Dibujo de capas de anotación**: lo hace la página de anotaciones (`annot.js`) con el evento del contrato, no este visor.

## Cifras

- Paquete con brotli (q11, lgwin 24, suma por archivo): 714 854 B. Con solo el componente eran 633 940 B.
- Binario (`cargo build --release`): 5 434 960 B sin visor de PDF; 6 100 488 B con el componente; 6 179 320 B con el visor completo (+78 832 B).
- Memoria y CPU: pendiente de medir con `tools/medir/medir.py` antes de aceptar el visor como definitivo (`docs/pruebas.md`).
- Pruebas: `cargo test` (con las de `vault/pdf.rs`) y una carga del visor en Chromium sin ventana (2 páginas, capa de texto, eventos, zoom, `?file=` rechazado).
