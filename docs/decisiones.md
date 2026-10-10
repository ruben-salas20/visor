# Decisiones tomadas

Cada punto se midió o se acordó con el usuario. Para cambiar uno, trae datos nuevos y consúltalo antes.

Cada decisión tiene su ADR en [`docs/adr/`](adr/).

- [0001. wry en lugar de GPUI](adr/0001-wry-no-gpui.md): wry da el webview; en Wayland exige un contenedor GTK.
- [0002. Un webview por cada página viva](adr/0002-un-webview-por-pagina.md): Cada página viva tiene su webview, sin iframes.
- [0003. Panel lateral en GTK nativo](adr/0003-panel-gtk-nativo.md): El panel es GTK nativo (`sidebar.rs`), no un webview.
- [0004. Iconos dibujados con cairo, ninguno del tema](adr/0004-iconos-con-cairo.md): Ningún icono sale del tema: se dibujan con cairo (`icons.rs`).
- [0005. Carpetas abiertas guardadas por bóveda](adr/0005-carpetas-abiertas-por-boveda.md): El árbol recuerda qué carpetas estaban abiertas, en `open_folders`.
- [0006. Un único WebContext](adr/0006-un-unico-webcontext.md): Los dos webviews comparten un solo `WebContext`.
- [0007. La bóveda se sirve por HTTP en 127.0.0.1](adr/0007-boveda-por-http.md): Servidor local con puerto fijo, clave secreta y control de `Host`.
- [0008. DMA-BUF activado](adr/0008-dma-buf-activado.md): DMA-BUF sigue activado por el rendimiento de Three.js.
- [0009. Caché de WebKit en modo WebBrowser y sin límite](adr/0009-cache-webbrowser.md): La caché usa el modo `WebBrowser` y no tiene límite.
- [0010. Cerrar la ventana cierra la app](adr/0010-cerrar-es-cerrar.md): Cerrar la ventana termina el proceso.
- [0011. Pestañas mixtas con un máximo de 3 páginas vivas](adr/0011-pestanas-mixtas-max-3.md): Máximo 3 páginas vivas (`MAX_LIVE`); las demás pestañas se descargan.
- [0012. Cada lado tiene sus pestañas](adr/0012-pestanas-por-lado.md): Cada lado tiene su fila de pestañas en GTK nativo, como Obsidian.
- [0013. Un webview nuevo se crea oculto](adr/0013-webview-oculto-hasta-committed.md): Se muestra al empezar a cargar (`Committed`), no antes.
- [0014. Arrastrar desde el panel con una capa GTK invisible](adr/0014-arrastrar-desde-el-panel.md): Una capa GTK recibe el arrastre; la web no lo ve.
- [0015. Unir los lados sin recargar](adr/0015-unir-los-lados.md): Unir pasa las pestañas de la derecha a la izquierda sin recargarlas.
- [0016. Posición de lectura al ocultar una pestaña](adr/0016-posicion-de-lectura.md): Se guarda la posición de lectura de la pestaña que se oculta.
- [0017. Espacio de trabajo por bóveda](adr/0017-espacio-de-trabajo-por-boveda.md): `workspaces` guarda pestañas, división y foco de cada bóveda.
- [0018. Barra de direcciones oculta por defecto](adr/0018-barra-oculta-por-defecto.md): La barra empieza oculta (`bar_shown`); los botones de anotaciones están en la fila de pestañas.
- [0019. Un HTML suelto se abre como pestaña, sin crear bóveda](adr/0019-pestana-suelta.md): Un `.html` fuera de toda bóveda conocida se abre como pestaña suelta.
- [0020. Ctrl+| además de Ctrl+\ para dividir](adr/0020-ctrl-barra-vertical.md): Dos atajos para dividir la ventana.
- [0021. Vigilancia de archivos](adr/0021-vigilancia-de-archivos.md): `notify` avisa al abrir; las escrituras propias se reconocen por hash.
- [0022. Enlaces](adr/0022-enlaces.md): http/https navegan en visor; `mailto:` va a `xdg-open`; el resto se ignora.
- [0023. Pantalla completa](adr/0023-pantalla-completa.md): Oculta panel, pestañas, barra y el otro lado.
- [0024. Tema de Omarchy](adr/0024-tema.md): Los colores se leen de Omarchy y llegan a las páginas como `--visor-*`.
- [0025. Anotaciones ancladas por cita y elemento](adr/0025-anotaciones-ancladas.md): El texto se ancla por cita con contexto; Rust es el único que escribe.
- [0026. CodeMirror 6 ligero con vista previa en vivo](adr/0026-codemirror-con-vista-previa.md): El editor de Ctrl+E es CodeMirror 6; solo se carga al editar.
- [0027. Sin @codemirror/lang-markdown](adr/0027-sin-lang-markdown.md): El lenguaje Markdown se monta con `@lezer/markdown` y GFM.
- [0028. Regenerar el paquete de CodeMirror](adr/0028-regenerar-el-paquete-de-codemirror.md): `npm ci && npm run build` en `tools/codemirror/`.
- [0029. Perfil de compilación: panic abort y opt-level 3](adr/0029-perfil-de-compilacion.md): `panic = "abort"` y `opt-level = 3`.
- [0030. Librerías de assets comprimidas con brotli](adr/0030-assets-con-brotli.md): `assets/` va con brotli; `build-override` optimiza el script de compilación.
- [0031. Mermaid completo](adr/0031-mermaid-completo.md): Se usa la versión completa de Mermaid (1,15 MB comprimido).
- [0032. Paquetes de GStreamer para vídeo](adr/0032-requisitos-de-gstreamer.md): El vídeo web y MSE piden paquetes de GStreamer.
- [0033. webkit2gtk en la misma versión exacta que wry](adr/0033-webkit2gtk-misma-version-que-wry.md): El pin `=2.0.2` copia el que exige wry 0.57.
