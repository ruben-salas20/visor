# 0022. Enlaces

**Estado:** Aceptada

## Contexto

Las páginas contienen enlaces de varios tipos.

## Decisión

http y https navegan dentro de visor. `mailto:` va a `xdg-open`. El resto se ignora. `target="_blank"` abre una pestaña nueva a la vista. Ctrl+clic y clic central abren una al final de la fila, que no se carga hasta mostrarla (no gasta memoria). Se detectan en `decide-policy`, como hace Epiphany.

## Consecuencias

Una pestaña sin cargar no gasta memoria.
