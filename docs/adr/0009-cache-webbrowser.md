# 0009. Caché de WebKit en modo WebBrowser y sin límite

**Estado:** Aceptada

## Contexto

WebKit ofrece los modos de caché `WebBrowser` y `DocumentViewer`.

## Decisión

`WebBrowser`, sin límite de tamaño. Es el valor por defecto de WebKitGTK: el código no lo fija.

## Consecuencias

`DocumentViewer` libera ~700 MB al salir de webs pesadas, pero las páginas con librerías de CDN se recargan más lentas y fallan sin conexión. El tamaño de la caché en disco no afecta al arranque.
