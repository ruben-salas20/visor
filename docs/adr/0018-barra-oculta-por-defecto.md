# 0018. Barra de direcciones oculta por defecto

**Estado:** Aceptada

## Contexto

La barra de direcciones ocupa espacio.

## Decisión

La barra está oculta por defecto (`bar_shown`). Los botones de anotaciones viven en la fila de pestañas, que siempre se ve. Solo aparecen con una página de la bóveda activa: en webs, archivos sueltos y pestañas vacías no hacen nada.

## Consecuencias

Ctrl+Shift+B muestra u oculta la barra y se recuerda. Ctrl+L la muestra mientras se escribe.

## Antes

El campo se llamaba `bar_hidden`. No consta por qué se invirtió.
