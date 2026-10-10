# 0017. Espacio de trabajo por bóveda

**Estado:** Aceptada

## Contexto

La configuración guardaba solo el último archivo de cada bóveda (`last`).

## Decisión

Se guarda un espacio de trabajo por bóveda (`workspaces`): pestañas, división y lado con el foco. `last` solo se lee si la bóveda aún no tiene espacio de trabajo (`vault/config.rs`).
