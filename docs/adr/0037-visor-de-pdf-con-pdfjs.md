# 0037. Visor de PDF con PDF.js

**Estado:** Aceptada · **Fecha:** 2026-10-16

## Contexto

Las bóvedas guardan PDF junto a las notas (libros, ejercicios, apuntes). Desde #37 se abren desde el panel y con `[[archivo.pdf]]`, pero en el visor de PDF.js que trae WebKitGTK: vive en un marco de esquema `webkit-pdfjs-viewer://`, donde los scripts de visor no entran, así que no admitía las anotaciones de visor (#27).

La alternativa de 0 KB, cargar el PDF.js de WebKit desde una página de visor, no se probó: depende de la versión de WebKit del sistema y no existe en Windows (#35), donde WebView2 no trae PDF.js. PDF.js es el visor de Firefox, con licencia Apache-2.0.

## Decisión

PDF.js 6.4.299 (`pdfjs-dist`, versión exacta) con su visor `pdf_viewer.mjs`, empaquetado por `tools/pdfjs/build.mjs` en `assets/pdfjs/` (ver `tools/pdfjs/README.md`). Igual que el resto de páginas de visor, se sirve desde la bóveda y nunca desde una CDN.

- Un `.pdf` de la bóveda se abre en una página de visor (`src/vault/pdf.rs`). Esa página carga PDF.js desde `__visor__/pdfjs/`, y lee los bytes del mismo archivo con `?raw` (`http.rs`), que sirve peticiones por rangos para que PDF.js no tenga que bajar el archivo entero.
- `enableScripting: false`: el JavaScript incrustado en un PDF no se ejecuta. `isEvalSupported: false`: tampoco se compila código a partir de sus fuentes (CVE-2024-4367, corregido en 4.2, pero cerrarlo cuesta una opción).
- Vista: páginas continuas ajustadas al ancho de la ventana y multiplicadas por el zoom de visor (Ctrl+más, Ctrl+menos, Ctrl+0). En un PDF, Rust deja el zoom de WebKit en 1 y le pasa el suyo a la página (`page_calls::pdf_zoom`): PDF.js dibuja los lienzos al tamaño en que se ven, y el de WebKit los estiraría borrosos. La barra y el esquema crecen con el mismo factor; «Página N de M» con campo para escribir un número; Ctrl+F con Enter, Mayús+Enter y «N de M»; Esc cierra; el esquema del PDF solo aparece si tiene uno.
- «Abrir con otra aplicación» usa un mensaje nuevo sin ruta (`open_pdf_elsewhere`). Solo lo acepta una página de la bóveda (`trusted`), y Rust abre el PDF que muestra la vista, comprobado con `vault::pdf::file_for`; la ruta no la pone la página.
- Solo los PDF de la bóveda usan este visor. Un PDF fuera de toda bóveda se abre suelto, como un HTML suelto, en el visor de WebKit y sin anotaciones. Un PDF no es página (`vault::is_page`): no se indexa como nota ni se edita.
- Contrato para las anotaciones: al dibujarse cada capa de texto, la página lanza `visor:pdf-textlayer` con `{ page }`, y expone `window.__visorPdf = { ready, goToPage }`.
- Las cadenas del visor de PDF.js van en inglés (`GenericL10n` sin `locale/`, ver «Lo que queda fuera»); las de visor van con `tr`.

## Lo que queda fuera

- **Carpetas de PDF.js no empaquetadas**: `cmaps/`, `standard_fonts/`, `iccs/`, `quickjs*`, `*_nowasm_fallback.js`, `.map` y `legacy/`. De `wasm/` solo `openjpeg`, `jbig2` y `qcms`; de `web/images/` solo las que usa el CSS. Sin `cmaps/` los textos en CJK con codificaciones no incrustadas pueden salir mal, y sin `standard_fonts/` las fuentes estándar no incrustadas usan las del sistema. No se ha medido el efecto.
- **Idioma**: los textos propios del visor de PDF.js (menús, mensajes de búsqueda internos) salen en inglés. Un `locale/` añadiría archivos por idioma; se decidirá con datos.
- **Herramientas del visor completo de PDF.js** (`web/viewer.html`, el que trae WebKit): miniaturas, girar, modo presentación, modo de desplazamiento, propiedades del documento, imprimir y descargar dentro de visor. Se empaqueta solo `pdf_viewer.mjs`, el componente, para que la barra sea de visor (en español, con `tr`) y no choque con las anotaciones; imprimir y guardar van por «Abrir con otra aplicación».
- **Tema oscuro**: solo cambian los colores del visor (barra, esquema, fondo). Las páginas del PDF conservan su color de papel, como en cualquier lector.
- **Contraseña**: no hay diálogo para pedirla. Un PDF protegido muestra que no se pudo abrir.
- **Dibujo de capas de anotación**: lo hace la página de anotaciones (`annot.js`) con el evento del contrato, no este visor.

## Cifras

- Paquete con brotli (q11, lgwin 24, suma por archivo): 633 940 B, en línea con la referencia de unos 634 KB.
- Binario (`cargo build --release`): 5 434 960 → 6 098 600 bytes (+663 640 B), contando también las anotaciones sobre PDF.
- Memoria y CPU: pendiente de medir con `tools/medir/medir.py` antes de aceptar el visor como definitivo (`docs/pruebas.md`).
- Pruebas: `cargo test` (con las pruebas nuevas de `vault/pdf.rs`), `node --test tools/pdfjs/test/` (lógica del visor sin DOM) y `tools/excalidraw/test/drawing-format.test.mjs`.
