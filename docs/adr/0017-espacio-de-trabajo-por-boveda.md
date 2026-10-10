# 0017. Espacio de trabajo por bóveda

**Estado:** Aceptada

## Contexto

Cada bóveda debe reabrirse como se dejó.

## Decisión

La configuración guarda un espacio de trabajo por bóveda (`workspaces`): pestañas, división y lado con el foco.

## Consecuencias

Cada bóveda recuerda sus pestañas y su división.

## Antes

La configuración guardaba solo el último archivo de cada bóveda, en `last`. Ahora `last` solo se lee si la bóveda aún no tiene espacio de trabajo. El texto de decisiones.md no explica por qué cambió; el comentario del código dice que `last` es de antes de las pestañas.
