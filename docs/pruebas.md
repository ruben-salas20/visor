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

2026-10-12, commit `5636bd8` (#26, #28 y #29: abrir con otra aplicación, ventana de atajos con Ctrl+K y Ctrl+1…9 para elegir pestaña), enchufado, perfil `performance`. Dos rondas solo con el binario nuevo; la columna «Antes» es la referencia anterior (`08e97d2`), medida horas antes. El valor del resumen de `medir.py` y, entre paréntesis, el rango. Las filas sin columna «Antes» vienen de medidas anteriores y no se repitieron.

| Medida | Valor | Antes (`08e97d2`) |
|---|---|---|
| Arranque en frío, nota mínima (`minima.md`) | 229 ms (223–229) | 229 ms (225–229) |
| Arranque en frío, `estudio/física/ondas.md` (Mermaid, KaTeX, código) | 764 ms (733–764) | 762 ms (717–762) |
| Arranque en frío, HTML suelto | 209 ms (208–209) | 260 ms (224–260); 207–220 ms al repetir |
| Cambio de página, dentro de la app (con la CLI) | 9,3 ms (6,8–9,3); 35–86 ms | 11,2 ms (8,5–11,2); 37 ms |
| Pestaña nueva, dentro de la app (con la CLI) | 146 ms (139–146); 173 ms | 145 ms (142–145); 176 ms |
| Memoria en reposo, nota mínima | 150 MB (133–150) | 161 MB (159–161) |
| Memoria en reposo, `ondas.md` | 246 MB (241–246) | 261 MB (259–261) |
| Memoria en reposo, HTML suelto | 131 MB (123–131) | **142 MB** |
| Memoria con el editor abierto (nota mínima) | 163 MB (149–163) | 177 MB |
| CPU en reposo, nota mínima u `ondas.md` | 0–0,1 % | 0–0,07 % |
| Cada página viva extra (pestañas o vista dividida, máximo 3) | +60–65 MB con una nota, ~+90 MB con Three.js (2026-10-07) | |
| Un dibujo de Excalidraw abierto, frente a una nota | ~+45 MB en lectura; en edición, 240 MB frente a 207 MB de una nota (ADR 0036) | |
| Un dibujo con fórmulas LaTeX, frente a uno sin ellas | ~225 ms y ~170 KB la primera vez, ~70 ms al recargar; +2–5 MB (3 medidas, poco fiable) | |
| Three.js (2000 nudos instanciados) | 60 fps | 60 fps |
| Binario (`cargo build --release`, con `strip`) | 5 373 992 bytes | 5 361 712 bytes |

Los 12,3 KB del binario son el botón «Abrir con otra aplicación» (~4,8 KB) y la ventana de atajos con Ctrl+1…9 (~7,4 KB). La ventana de atajos no se crea hasta el primer Ctrl+K. La memoria sale 11–16 MB por debajo de la referencia anterior y la primera ronda dio 16 MB menos que la segunda con el mismo binario: es deriva, no un efecto del código (ver abajo). En la primera ronda, la CPU con `ondas.md` y los fps se descartaron por ventana oculta.

**Coste conocido, aceptado: +11,4 MB con un HTML suelto.** Sale igual en todas las repeticiones (130 → 141,5 MB) y no aparece con notas de la bóveda. Lo causan los tres botones de navegación visibles en la fila de pestañas (#22, `b9cc9c9`): el commit anterior da 130 MB, y el mismo commit con los botones creados pero sin colocar en la fila, también. No depende del icono (con botones de texto sube igual), del tooltip ni de que estén desactivados, y no carga glycin. Crece el *heap* del proceso principal (+16,7 MB) y bajan los búferes `memfd:gdk-wayland` (−5,5 MB). No es un búfer extra por redibujar: pasar el ratón por la fila no cambia la cifra de antes ni la de después. Con una bóveda, el panel lateral ya paga ese coste y los botones no añaden nada. La causa exacta en GTK3 sigue sin identificar.

La memoria absoluta es ~25 MB menor que en la referencia del 2026-10-11 con los dos binarios: es la deriva entre días descrita abajo, no un cambio del código.

En la referencia anterior (`5e67e74`, tres rondas con `6399c0b`), las dos primeras rondas daban más memoria con la nota mínima (+3–9 MB) y más tiempo de pestaña nueva (+4–12 ms); la tercera dio menos memoria con el binario nuevo y acercó la pestaña nueva (141,5 frente a 145,1 ms). Los rangos se solapan: es ruido, aunque la pestaña nueva conviene vigilarla en la próxima medida. Los 48 KB del binario son el botón de actualizar y `visor update` (~17 KB), las imágenes pegadas (~17 KB), las fórmulas (~6 KB) y el resto de cambios.

La memoria bajó ~80 MB respecto de la referencia anterior (217 MB con la nota mínima en `02727b2`) sin que cambiara el código: el binario de `02727b2`, medido el mismo día con el mismo método, da ~131 MB. La comparativa del README dio 215 MB esa tarde y la bajada apareció horas después, sin reiniciar. Se descartaron la consulta de actualizaciones, la pantalla, `alsa-plugins` y otra instancia de visor abierta (esta última baja la medida solo 3–5 MB). La causa sigue sin identificar: compara solo rondas alternadas de una misma ejecución, nunca cifras de días distintos.

Desde `f9a0320`, el cambio de página y la pestaña nueva se miden dentro de la app; las cifras de la CLI van aparte. No se comparan con las de referencias anteriores a ese commit (74 ms y 176–185 ms incluían la CLI y una espera de 50 ms).
