# U1: marco de la interfaz en HTML

Prototipo del problema 35 de visor. Comprueba si la interfaz (el «marco») puede escribirse en HTML y convivir con las páginas de estudio en la misma ventana.

## Resumen

Tres pruebas pasan sin reservas. Tres pasan con límites. Dos fallan: el arrastre desde el árbol y Ctrl+W con el foco en una página.

El problema principal: el puntero real no llega al marco en el entorno de prueba. Esto impide confirmar el arrastre. La causa no está encontrada.

## Cómo se probó

- Entorno: sway sin pantalla física (headless) en la unidad `u1-sway`. Sway es el gestor de ventanas que usa Wayland. Tenía su propio socket, así que no se abrió nada en el escritorio real.
- Orden: `timeout 200 ./pruebas/ejecutar.sh`, desde `spikes/u1-marco-html/`.
- Última ejecución: ejecución 13. Registro en `/tmp/claude-1000/u1-run13.txt`.
- Bóveda de prueba: 10 carpetas con 20 notas cada una, generadas al arrancar.

## Resultados

| Prueba | Resultado | Evidencia |
|---|---|---|
| a) Marco y dos páginas en su sitio | Funciona | Marco listo. El árbol tiene 200 elementos. Las dos páginas cargan por completo. Captura: `capturas/a-marco.png`. |
| b) Crear, activar y cerrar pestañas | Funciona | Abrir una nota deja 1 pestaña. «Nueva pestaña» deja 2. Cerrar deja 1. Captura: `capturas/b-pestanas.png`. |
| c) Abrir archivo desde el árbol | Funciona con límites | La página confirma la carga de `carpeta-03/nota-041.html`. La orden se lanzó con un clic simulado por JavaScript, no con el puntero. Captura: `capturas/c-abrir-arbol.png`. |
| d) Recolocación al cambiar el tamaño | Funciona | Con la ventana a 1000×650, la página ocupa el mismo hueco que el marco: x 260, y 64, 509×736. Al volver a 1280×800 también coincide. Captura: `capturas/d-recolocada.png`. |
| e) Página hostil | Funciona con límites | Se rechazaron 8 de 8 órdenes no permitidas o rutas fuera de la bóveda. El marco no cambió. La página siguió en `hostil.html` tras intentar `file:///etc/hostname`, `https://example.com/` y `window.open`. No hay registro directo de cada bloqueo; la prueba es indirecta. |
| f) Arrastre del árbol a una página | No funciona | Ningún evento del marco durante el arrastre (`eventos_del_marco_durante_arrastre: []`). El panel 1 sigue con 1 pestaña. Ni la vía HTML5 ni la vía por coordenadas se confirmó. |
| g1) Ctrl+W con el foco en la página | No funciona (inestable) | El panel 1 pasa de 1 pestaña a 1 pestaña (`pestanas_antes: 1`, `pestanas_despues: 1`). Pasó en las ejecuciones 9 y 11. Falló en las 10, 12 y 13. |
| g2) Ctrl+W con el foco en el marco | Funciona con límites | El total de pestañas pasa de 3 a 2. `wtype` devolvió éxito. El foco lo puso el programa, no un clic del usuario. |

## Tamaño del binario

`ls -l target/release/u1-marco-html`: 2363680 bytes (unos 2,3 MB), compilado en modo release.

Esto no es una medida de rendimiento. El encargo no pedía medir memoria, arranque ni CPU.

## Qué no se probó

- Lector de pantalla (Orca): no se probó.
- Arrastre con el puntero real, por HTML5 o por coordenadas: no se consiguió. La prueba f falló antes.
- Clic real del puntero sobre el árbol: no llega al marco en este entorno.
- Registro directo de los bloqueos de navegación: el código no lo guarda. Solo hay prueba indirecta.
- Escritorio Hyprland real: no se usó, por la norma del encargo.
- Rendimiento (memoria, arranque, CPU): no se midió.
- Varias ventanas, bóvedas reales y notas reales: no se probaron. Solo se usó la bóveda generada.
- Windows y macOS: no se probaron.

## Problemas sin resolver

1. **El puntero no llega al marco.** Síntoma exacto: `eventos_del_marco_tras_clic_de_prueba: []`. Los clics sobre las páginas sí llegan. La causa no se encontró.
2. **g1 es inestable.** El mismo código da resultados distintos entre ejecuciones (2 de 5 pasan). No hay explicación todavía.
3. **`bounds()` no lee la posición.** En la prueba d, la posición se comprobó con `allocation()`. Esto no cambia el resultado, pero conviene saberlo.

## Revisión (2026-10-19)

Causa probable del problema 1: el entorno, no el marco. En el sway sin pantalla (`WLR_LIBINPUT_NO_DEVICES=1`), `swaymsg -t get_seats` muestra `seat0` **sin dispositivos**, así que no hay puntero que entregar a las ventanas. `wtype` sí crea un teclado virtual. Un clic manual con `swaymsg seat - cursor` sobre «+ nueva pestaña» y sobre un archivo del árbol no cambió nada. El puntero, el arrastre y el foco (f, g1) se prueban a mano en un escritorio real.

## Siguiente paso propuesto

Aislar por qué el puntero no llega al marco. Un buen primer paso es repetir el clic en una ventana GTK vacía dentro del mismo sway. Si esa ventana sí recibe el clic, el problema está en el marco. Esto no está hecho.

## Después de la revisión (2026-10-19)

- **Prueba a mano en Hyprland (usuario):** clic en el árbol, arrastre del árbol a una página, Ctrl+W con el foco en una página y redimensionar funcionan. Al encoger la ventana, el contenido salía estrechado y quedaba una franja vacía abajo.
- **Arreglo del encogido:** el marco es ahora el hijo principal de un `gtk::Overlay` (sigue el tamaño de la ventana) y las páginas van en un `gtk::Fixed` superpuesto con `set_overlay_pass_through`. Comprobado en sway sin pantalla a escala 1,25 y a media pantalla, y por el usuario en Hyprland.
- **Tres marcos con el mismo protocolo:** `--marco simple` (`web/marco.html`), `--marco react` (React 19 + shadcn/ui + Tailwind 4, fuente en `marcos/react/`) y `--marco svelte` (Svelte 5 + shadcn-svelte, fuente en `marcos/svelte/`). Los tres pasan a–e. Capturas con prefijo `react-` y `svelte-`.
- **Un solo `WebContext`** para todas las vistas, como visor: con uno por vista, wry arranca más procesos y la memoria no es comparable.
- **Medidas** (`pruebas/medir.py` y `pruebas/medir_visor.py`): ver la tabla «Resultados» de `docs/plan-multiplataforma.md`.
- **Decisión del usuario:** opción A con React, cargando solo los componentes que se ven.
