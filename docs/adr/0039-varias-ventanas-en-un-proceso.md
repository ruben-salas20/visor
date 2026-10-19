# 0039. Varias ventanas en un proceso

**Estado:** Aceptada (2026-10-18, issue #39)

## Contexto

Con dos monitores, el usuario quiere una ventana de visor en cada pantalla. Cada ventana debe tener su bóveda y sus pestañas, y recordarse.

Hasta ahora había una ventana por proceso y un proceso por socket. Un segundo `visor` decía «visor ya está abierto» y terminaba.

## Decisión

Decidido con el usuario:

- **Un solo proceso abre todas las ventanas.** Comparten el socket, el `WebContext` (ADR 0006), la configuración y el tema. Varias instancias costarían el motor entero otra vez, y los agentes tendrían que elegir socket.
- **Un segundo `visor` abre otra ventana.** Lo pide por el socket con la orden `new_window`. La ventana nueva muestra la bóveda de la ventana con foco, con una pestaña vacía.
- **Cada ventana tiene su bóveda y sus pestañas.** Dos ventanas pueden mostrar la misma bóveda con pestañas independientes.
- **`config.json` guarda la lista de ventanas** (`windows`): bóveda, pestañas, tamaño, ancho del panel y división. Al arrancar se abren todas. Hyprland decide en qué monitor va cada una: en Wayland la app no puede elegirlo.
- **Al cerrar:**
  - Ctrl+Q cierra todas las ventanas y las recuerda.
  - Cerrar una ventana con otras abiertas la olvida, pero su bóveda conserva esas pestañas como las últimas (`workspaces`).
  - Cerrar la última ventana la recuerda y termina el proceso.
- **Cada bóveda tiene su origen** (ADR 0007): su propia dirección de bucle local y su propia clave. Así una página de una bóveda no puede leer otra. Se aceptó perder el `localStorage` de las bóvedas que no conservan 127.0.0.1.
- **Los agentes hablan con la ventana con foco**, la última que usó la persona. Con `--window N` (en MCP, `window`) eligen otra. `open` y `close` de una ruta van a la ventana que ya muestra su bóveda. `visor status` lista las ventanas.
- **`MAX_LIVE` cuenta por ventana.** Cada ventana tiene hasta 3 páginas vivas.

Decidido sin consultar:

- Los números de ventana empiezan en 1 y no se reutilizan.
- Los ajustes de la persona son comunes y se aplican a todas las ventanas: tema, zoom, barra, idioma, buscador, archivos no visibles y actualizaciones.
- El título es «Visor — <bóveda>».

## Código

- `ui/visor.rs` tiene el proceso: el bucle de eventos, las ventanas, el cierre, Ctrl+Q y el reinicio. Una ventana es un `App` (`ui/app.rs`).
- Lo común está en `ui/common.rs`. Los ajustes que cambia una ventana llegan a las demás comparando `Settings` antes y después de cada evento.
- Cada evento lleva su ventana (`Routed`). La ventana 0 es el proceso.
- `ui/windows.rs` es el modelo sin GTK, con pruebas.
- Los ids de vista de cada ventana empiezan en `ventana << 40`, para que no choquen entre ventanas.

## Consecuencias

- Las medidas de una y de dos ventanas están pendientes; las pide `docs/pruebas.md` y se hacen con la autorización del usuario.
- El reinicio tras una actualización solo comprueba si hay notas sin guardar en la ventana que lo pide.
- Mover o borrar un archivo desde el panel de una ventana no reapunta las pestañas de otra ventana con la misma bóveda. Esa ventana ve el cambio por su vigilante de archivos, como si viniera de fuera.
- Si dos ventanas editan la misma nota, gana el último guardado. Cada ventana trata lo que escribe la otra como un cambio externo, igual que la edición de un agente.
