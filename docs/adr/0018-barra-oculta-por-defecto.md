# 0018. Barra de direcciones oculta por defecto

**Estado:** Aceptada

## Decisión

La barra empieza oculta (`bar_shown`) para ahorrar espacio; el campo se llamaba antes `bar_hidden` y no consta por qué se invirtió. Ctrl+Shift+B la muestra u oculta y se recuerda; Ctrl+L la muestra mientras se escribe.

Los botones de anotaciones viven en la fila de pestañas, que siempre se ve. Solo aparecen con una página de la bóveda activa: en webs, archivos sueltos y pestañas vacías no hacen nada.
