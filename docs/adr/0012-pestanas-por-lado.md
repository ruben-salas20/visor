# 0012. Cada lado tiene sus pestañas

**Estado:** Aceptada

## Decisión

Cada lado de la vista dividida tiene sus pestañas, como Obsidian. Pestañas, barra y botones de anotaciones son GTK nativo (`ui/panes.rs`, `ui/bar.rs`), igual que el panel. Los atajos y las órdenes de los agentes (`open`, `reload`, `console`, `notes`) actúan sobre el lado con el foco: lo que la persona está mirando.
