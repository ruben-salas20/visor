# 0029. Perfil de compilación: panic abort y opt-level 3

**Estado:** Aceptada

## Contexto

Se buscaba un binario pequeño sin perder velocidad al renderizar Markdown.

## Decisión

`panic = "abort"` y `opt-level = 3`.

## Cifras

`opt-level = "s"` ahorraba 380 KB pero renderizaba Markdown un 8–15 % más lento; `"z"`, un 50 % más lento.
