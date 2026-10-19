# U1 en Windows: marco HTML y dos páginas en WebView2

Prototipo del issue 35. Una ventana winit con tres webviews hijos de WebView2 (wry 0.57, `build_as_child`) y un solo `WebContext`:

- el marco React de `spikes/u1-marco-html/web/react/`, que ocupa toda la ventana;
- dos páginas, colocadas sobre el hueco que el marco anuncia con `recolocar`.

El contenido se sirve desde `../u1-marco-html/web` con un servidor local. La bóveda de 200 notas se genera al arrancar en `web/boveda/`, que git ignora.

Solo se ha ejecutado en GitHub Actions (`.github/workflows/windows-u1.yml`, runner `windows-2025`, WebView2 Runtime 154.0.4258.62).

## Uso

```
u1-windows --auto <salida.json> --prueba a|b|c|d
```

Termina solo. Código de salida: 0 si la prueba pasa, 1 si falla, 3 si se agota el tiempo (60 s) y 4 si el hilo principal se bloquea más de 5 s.

## Resultados (ejecuciones 39265938803 y 39266487153)

| Prueba | Resultado | Evidencia |
|---|---|---|
| a) Marco y dos páginas cargan, la ventana responde | Funciona | 5 de 5 ok en cada ejecución (10 de 10), ningún cuelgue. `listo` del marco entre 1,3 y 3,6 s; las dos notas, unos 50 ms después. `evaluate_script` responde en las tres vistas al cargar y 3 s después. |
| b) Página hostil | Funciona | Rust rechaza los 8 mensajes y registra el motivo: 7 «acción no permitida desde una página» y 1 «ruta fuera de la bóveda». El estado del marco no cambia. La URL confirmada por COM sigue en `/hostil.html`. El gestor de navegación bloqueó `https://example.com/`, y el de ventanas nuevas negó `window.open`. `file:///etc/hostname` no llegó al gestor de navegación ni cambió la URL confirmada. Lo más probable es que WebView2 lo frene antes, porque una página http no puede abrir `file://`; no se comprobó en sus registros. |
| c) Memoria | Funciona | Marco + 2 páginas: 9 procesos, 166 MB privados, 444 MB de working set. Marco + 5 páginas: 12 procesos, 243 MB privados, 612 MB de working set. Cada página extra cuesta **25,5 y 26,2 MB privados** en las dos ejecuciones (55–56 MB de working set) y añade un proceso de render, aunque sea del mismo origen. |
| d) Redimensionar | Funciona | 1000×650 → 1280×800 → 1000×650 → 700×450, con escala 1,0. En cada paso la ventana toma el tamaño pedido, el marco ocupa toda la ventana y los `bounds()` de cada página coinciden al píxel con su recta. Encoger funciona, a diferencia de Linux. |

El working set suma memoria compartida entre procesos: para comparar, usa los bytes privados.

## Qué no se probó

- Clic y arrastre reales del ratón ni el teclado. Las pruebas solo usan IPC y `evaluate_script`.
- Un lector de pantalla (Narrador o NVDA).
- Windows de escritorio (10 u 11). El runner es Windows Server 2025, con escala 1,0: no se probó con 125 % ni 150 %.
- El orden de apilado de los webviews (que las páginas queden visibles encima del marco). No hay capturas.
