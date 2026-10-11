# Probar y medir

## Instancia aislada

El CLI habla con la instancia que escucha en el socket. Si el usuario tiene visor abierto, un `visor open` sin aislar abre páginas en su ventana y escribe en su `~/.config/visor`. Aísla siempre:

```sh
export TMP=$(mktemp -d)
export VISOR_SOCKET=$XDG_RUNTIME_DIR/visor-prueba.sock   # corta: los sockets Unix admiten ~108 caracteres
export XDG_CONFIG_HOME=$TMP/cfg XDG_STATE_HOME=$TMP/state XDG_DATA_HOME=$TMP/data XDG_CACHE_HOME=$TMP/cache
target/release/visor & echo $! > $TMP/pid      # ciérrala con kill $(cat $TMP/pid)
```

Cierra solo por PID: `pkill -f` con un patrón presente en tu propia orden mata tu shell, y con la ruta relativa alcanza la instancia del usuario.

## Interactuar sin ratón

- Teclas reales (Hyprland usa configuración en Lua):
  `hyprctl dispatch 'hl.dsp.send_key_state({ mods = "CTRL", key = "E", state = "down", window = "address:0x…" })'`, y lo mismo con `state = "up"`. La dirección sale de `hyprctl clients -j` filtrando por `pid`.
- Captura: `grim -g "x,y anchoxalto"` con la geometría de `hyprctl clients -j`.
- Gestos imposibles de enviar (selección de texto, clics): una página de prueba en la bóveda que los reproduzca por script (`dispatchEvent`, `.click()` dentro de `document.querySelector("visor-capa").shadowRoot`); lee el resultado con `visor console`.
- URL base del servidor (lleva una clave nueva en cada arranque): una página con `visor.send({ href: location.href })` y `visor events --limit 1`.

## Medir

```sh
cp target/release/visor /tmp/visor-antes        # cargo sobrescribe el binario al compilar
# … cambia y compila …
cp target/release/visor /tmp/visor-despues
tools/medir/medir.py --rondas 1 /tmp/visor-antes /tmp/visor-despues
tools/medir/medir.py --resumen ${TMPDIR:-/tmp}/visor-medir/resultados-….jsonl
```

`medir.py` aísla la instancia, copia la bóveda `tools/medir/boveda/`, alterna los binarios y mide arranque en frío, memoria (PSS del árbol de procesos, 10 s tras cargar), CPU en reposo (30 s), cambio de página, pestaña nueva y editor. Three.js escribe sus fps en la consola cada segundo (`visor console`).

El cambio de página y la pestaña nueva se miden de dos formas. `cambio_pagina_ms` y `pestana_nueva_ms` son lo que tarda la carga dentro de la app: con `VISOR_TIEMPOS=1`, `visor open` añade `ms` a su respuesta. Las variantes `_cli_ms` incluyen además el arranque de la CLI (~25 ms), como lo ve un agente. Para comparar con otra app, usa las primeras. `medir.py` también pone `VISOR_NO_UPDATE=1`, para que ninguna ronda consulte GitHub.

- Cada ronda dura ~2 min por binario y abre ventanas en el escritorio activo: no uses el equipo mientras tanto. Rondas: 1 para comprobar un cambio, 2 antes de un push que reúne varios cambios y 3 antes de sacar un release. Repite con una ronda más si una cifra se sale del ruido.
- La ventana debe estar visible en el escritorio activo: WebKit frena las páginas ocultas y falsea la CPU. El resumen descarta esas rondas. Mover el ratón sobre la ventana la redibuja y sube la CPU a ~2 %.
- Memoria en PSS, no RSS (el RSS cuenta doble).
- La página manda en la memoria: una nota con Mermaid y KaTeX ocupa ~100 MB más que una de un párrafo con el mismo binario. Compara con la misma página.
- El ruido supera a menudo el efecto: con diferencias de pocos ms, compara solo rondas alternadas. El arranque con `ondas.md` es bimodal con cualquier binario (~670 y ~725 ms).
- El perfil de energía manda: con el mismo binario, el arranque en frío va de ~220 ms (`performance`) a ~310 ms (`balanced`, enchufado), ~800 ms (batería) y ~1,1 s (`power-saver`). Compara solo en las mismas condiciones (`powerprofilesctl get`, enchufado o no).

## Comparar con Obsidian y Chromium

```sh
tools/medir/comparar.py --prueba                 # una medida por app, con capturas en $TMPDIR/visor-medir/prueba/
tools/medir/comparar.py --rondas 5               # ~25 min sin usar el equipo
tools/medir/comparar.py --resumen $TMPDIR/visor-medir/comparar-….jsonl   # escribe docs/img/cifras.json
tools/medir/graficas.py                          # docs/img/graficas/*.svg
node tools/video/grabar.mjs && tools/video/demo.sh && tools/video/unir.sh   # GIF del README
```

Cada app usa su propio perfil temporal (nunca `~/.config/obsidian` ni el Chromium del usuario), calentado por un primer arranque que no cuenta, y el orden se alterna entre rondas. El arranque es hasta que aparece la ventana, igual para las tres; no incluye dibujar la página. `demo.sh` abre ventanas flotantes unos 30 s por idioma y añade a Hyprland, hasta que recargue, una regla para la etiqueta `visordemo` (opacas, sin borde).

## Referencia

2026-10-11, commit `5e67e74` (#6, #7, #14, #16 y #17: imágenes por URL, LaTeX e imágenes pegadas en dibujos, archivos ocultos, Text Elements, actualizar a mano, versión en el panel), enchufado, perfil `performance`. Tres rondas alternadas con `6399c0b`, la referencia anterior; mediana y, entre paréntesis, el rango.

| Medida | Valor | Antes (`6399c0b`) |
|---|---|---|
| Arranque en frío, nota mínima (`minima.md`) | 219 ms (217–222) | 224 ms (223–227) |
| Arranque en frío, `estudio/física/ondas.md` (Mermaid, KaTeX, código) | 758 ms (754–760) | 786 ms (770–805) |
| Arranque en frío, HTML suelto | 210 ms (205–233) | 213 ms (201–214) |
| Cambio de página, dentro de la app (con la CLI) | 9,2 ms; 75 ms | 9,6 ms; 46 ms |
| Pestaña nueva, dentro de la app (con la CLI) | 145 ms (137–146); 172 ms | 133 ms (133–142); 160 ms |
| Memoria en reposo, nota mínima | 186 MB (183–186) | 183 MB (177–186) |
| Memoria en reposo, `ondas.md` | 289 MB | 288 MB |
| Memoria en reposo, HTML suelto | 154 MB | 154 MB |
| Memoria con el editor abierto (nota mínima) | 207 MB | 204 MB (204–207) |
| CPU en reposo, nota mínima u `ondas.md` | 0–0,07 % | 0,03–0,07 % |
| Cada página viva extra (pestañas o vista dividida, máximo 3) | +60–65 MB con una nota, ~+90 MB con Three.js (2026-10-07) | |
| Un dibujo de Excalidraw abierto, frente a una nota | ~+45 MB en lectura; en edición, 240 MB frente a 207 MB de una nota (ADR 0036) | |
| Un dibujo con fórmulas LaTeX, frente a uno sin ellas | ~225 ms y ~170 KB la primera vez, ~70 ms al recargar; +2–5 MB (3 medidas, poco fiable) | |
| Three.js (2000 nudos instanciados) | 60 fps | 60 fps |
| Binario (`cargo build --release`, con `strip`) | 5 333 800 bytes | 5 285 088 bytes |

Las dos primeras rondas daban más memoria con la nota mínima (+3–9 MB) y más tiempo de pestaña nueva (+4–12 ms); la tercera dio menos memoria con el binario nuevo y acercó la pestaña nueva (141,5 frente a 145,1 ms). Los rangos se solapan: es ruido, aunque la pestaña nueva conviene vigilarla en la próxima medida. Los 48 KB del binario son el botón de actualizar y `visor update` (~17 KB), las imágenes pegadas (~17 KB), las fórmulas (~6 KB) y el resto de cambios.

La memoria bajó ~80 MB respecto de la referencia anterior (217 MB con la nota mínima en `02727b2`) sin que cambiara el código: el binario de `02727b2`, medido el mismo día con el mismo método, da ~131 MB. La comparativa del README dio 215 MB esa tarde y la bajada apareció horas después, sin reiniciar. Se descartaron la consulta de actualizaciones, la pantalla, `alsa-plugins` y otra instancia de visor abierta (esta última baja la medida solo 3–5 MB). La causa sigue sin identificar: compara solo rondas alternadas de una misma ejecución, nunca cifras de días distintos.

Desde `f9a0320`, el cambio de página y la pestaña nueva se miden dentro de la app; las cifras de la CLI van aparte. No se comparan con las de referencias anteriores a ese commit (74 ms y 176–185 ms incluían la CLI y una espera de 50 ms).
