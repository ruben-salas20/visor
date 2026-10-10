# 0015. Unir los lados sin recargar

**Estado:** Aceptada

## Contexto

Se puede volver de la vista dividida a un solo lado.

## Decisión

Unir los lados pasa las pestañas de la derecha a la izquierda. Los webviews cambian de contenedor sin recargarse. El divisor se guarda como proporción del ancho (`split_ratio`).

## Consecuencias

Las páginas conservan su estado al unir.
