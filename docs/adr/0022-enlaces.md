# 0022. Enlaces

**Estado:** Aceptada

## Decisión

- http y https navegan dentro de visor. `mailto:` va a `xdg-open`. El resto se ignora.
- `target="_blank"` abre una pestaña nueva a la vista.
- Ctrl+clic y clic central abren una al final de la fila que no se carga hasta mostrarla, así que no gasta memoria.
- Se detectan en `decide-policy` (`ui/page_view.rs`), como hace Epiphany.
