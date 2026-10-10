# 0012. Cada lado tiene sus pestañas

**Estado:** Aceptada

## Contexto

La ventana puede dividirse en dos lados.

## Decisión

Cada lado tiene sus pestañas, como Obsidian. Las pestañas, la barra y los botones de anotaciones son GTK nativo (`panes.rs` y `bar.rs`), igual que el panel. La CLI y los atajos actúan sobre el lado con el foco.

## Consecuencias

Las órdenes de los agentes (`open`, `reload`, `console`, `notes`) actúan sobre lo que la persona está mirando.
