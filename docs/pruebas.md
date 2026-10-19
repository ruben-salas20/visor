# Probar y medir

## Instancia aislada

El CLI habla con la instancia que escucha en el socket. Si el usuario tiene visor abierto, un `visor open` sin aislar abre páginas en su ventana y escribe en su `~/.config/visor`. Aísla siempre:

```sh
export TMP=$(mktemp -d)
export VISOR_SOCKET=$XDG_RUNTIME_DIR/visor-prueba.sock   # corta: los sockets Unix admiten ~108 caracteres
export XDG_CONFIG_HOME=$TMP/cfg XDG_STATE_HOME=$TMP/state XDG_DATA_HOME=$TMP/data XDG_CACHE_HOME=$TMP/cache
target/release/visor & echo $! > $TMP/pid      # ciérrala con kill $(cat $TMP/pid)
```

Para que la ventana no aparezca en el escritorio del usuario, ábrela en un sway sin pantalla. Su ventana cuenta como visible, así que WebKit no frena la página:

```sh
printf 'output HEADLESS-1 resolution 1920x1080\nxwayland disable\nexec sh -c "echo $WAYLAND_DISPLAY > %s/display"\n' $TMP > $TMP/sway.conf
systemd-run --user --quiet --collect --unit=visor-prueba-sway -E WLR_BACKENDS=headless -E WLR_LIBINPUT_NO_DEVICES=1 \
  -E SWAYSOCK=$TMP/sway.sock -E XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR sway -c $TMP/sway.conf   # ciérralo con systemctl --user stop visor-prueba-sway
sleep 1; test -s $TMP/display && export WAYLAND_DISPLAY=$(cat $TMP/display) SWAYSOCK=$TMP/sway.sock   # antes de arrancar visor
```

Lanzado como hijo de la shell de un agente, sway recibe SIGKILL a los pocos milisegundos (visto el 2026-10-18); como servicio de usuario, no. Si `$TMP/display` no existe, no sigas: visor iría a la pantalla del usuario.

Cierra solo por PID: `pkill -f` con un patrón presente en tu propia orden mata tu shell, y con la ruta relativa alcanza la instancia del usuario.

## Interactuar sin ratón

- En sway sin pantalla, la ventana de visor tiene el foco. Teclas: `wtype -P F24 -p F24 -s 100 -M ctrl -P e -p e -m ctrl` (sway pierde la primera tecla de cada `wtype` mientras carga su mapa de teclado; F24 no hace nada). Captura: `grim` sin geometría. Ventanas: `swaymsg -t get_tree`.
- Teclas reales en Hyprland (Hyprland usa configuración en Lua):
  `hyprctl dispatch 'hl.dsp.send_key_state({ mods = "CTRL", key = "E", state = "down", window = "address:0x…" })'`, y lo mismo con `state = "up"`. La dirección sale de `hyprctl clients -j` filtrando por `pid`.
- Captura en Hyprland: `grim -g "x,y anchoxalto"` con la geometría de `hyprctl clients -j`.
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

- Cada ronda dura ~2 min por binario. `medir.py` arranca su propio sway sin pantalla y abre allí las ventanas: el escritorio queda libre, pero lo que hagas compite por la CPU y mueve los tiempos de arranque. Con `--escritorio` mide en el escritorio activo de Hyprland, como antes del 2026-10-18; entonces no uses el equipo. Las cifras de un entorno no se comparan con las del otro. Rondas: 1 para comprobar un cambio, 2 antes de un push que reúne varios cambios y 3 antes de sacar un release. Repite con una ronda más si una cifra se sale del ruido.
- La ventana debe estar visible: WebKit frena las páginas ocultas y falsea la CPU. El resumen descarta esas rondas. En sway sin pantalla siempre lo está; en Hyprland, mover el ratón sobre la ventana la redibuja y sube la CPU a ~2 %.
- Memoria en PSS, no RSS (el RSS cuenta doble).
- La página manda en la memoria: una nota con diagramas y KaTeX ocupa ~100 MB más que una de un párrafo con el mismo binario (con Mermaid; con Pele, ~30–60 MB en sway). Compara con la misma página.
- El ruido supera a menudo el efecto: con diferencias de pocos ms, compara solo rondas alternadas. El arranque con `ondas.md` es bimodal con cualquier binario (~670 y ~725 ms).
- El perfil de energía manda: con el mismo binario, el arranque en frío va de ~220 ms (`performance`) a ~310 ms (`balanced`, enchufado), ~800 ms (batería) y ~1,1 s (`power-saver`). Compara solo en las mismas condiciones (`powerprofilesctl get`, enchufado o no).
- El turbo también: tras arrancar con la batería baja y cargando, el firmware lo dejó desactivado y el arranque subió de ~214 a ~344 ms con `performance` (2026-10-12). Antes de medir, `cat /sys/devices/system/cpu/intel_pstate/no_turbo` debe dar `0`; si da `1`, actívalo con `echo 0 | sudo tee …/no_turbo`.

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

### En sway sin pantalla

#### Varias ventanas (#39)

2026-10-19, rama `feat/39-ventanas` frente a `main` (`c77d2f1`). Enchufado, perfil `performance`, en sway sin pantalla.

Con una ventana, una ronda alternada de `medir.py` no muestra coste: todas las diferencias caen dentro del ruido. Por ejemplo, el binario de `main` dio 131,9 y 143,2 MB con la nota mínima en dos ejecuciones seguidas. El binario pesa 5 319 464 bytes frente a 5 244 672 (+73 KB).

Dos ventanas, medidas con un guion propio. Usa el PSS de `medir.py`, la nota mínima en cada ventana y 10 s de espera tras cargar. Son tres rondas:

| Medida | Resultado |
|---|---|
| Una ventana | 140–155 MB |
| Dos ventanas en un proceso | 185–211 MB |
| Coste de la segunda ventana | +46–56 MB |
| Dos instancias separadas, como alternativa | 248–258 MB |
| Abrir la segunda ventana con `visor` (hasta que la orden termina) | 61–95 ms |

Dos instancias cuestan unos 50–60 MB más que dos ventanas en un proceso: cada instancia carga su propio proceso de red y su propia interfaz. Es la razón de la opción elegida en el ADR 0039.

#### Pele (#40)

2026-10-18, #40 (diagramas con Pele en vez de Mermaid), enchufado, perfil `performance`, medido con `medir.py` en sway sin pantalla. Dos rondas alternadas con el binario de `8bd1820` en la misma ejecución: intervalo de las dos rondas. No se compara con la tabla de Hyprland de abajo.

| Medida | Pele | Antes (`8bd1820`, Mermaid) |
|---|---|---|
| Peso del binario | 5,21 MB | 6,18 MB |
| Arranque en frío, nota mínima (`minima.md`) | 217–228 ms | 207–213 ms |
| Arranque en frío, `estudio/física/ondas.md` (diagrama, KaTeX, código) | 420–432 ms | 684–717 ms |
| Arranque en frío, HTML suelto | 202–245 ms | 205–214 ms |
| Arranque en frío, PDF | 259–270 ms | 253–264 ms |
| Cambio de página, dentro de la app | 7,8–8,9 ms | 7,2–7,7 ms |
| Pestaña nueva, dentro de la app | 134–138 ms | 136–145 ms |
| Memoria en reposo, nota mínima | 125–164 MB | 156–164 MB |
| Memoria en reposo, `ondas.md` | 190 MB | 249–259 MB |
| Memoria en reposo, HTML suelto | 145–148 MB | 147–148 MB |
| Memoria en reposo, PDF | 210–239 MB | 227–237 MB |
| Memoria con el editor abierto (nota mínima) | 145–176 MB | 168–174 MB |
| CPU en reposo, nota mínima u `ondas.md` | 0,03 % | 0–0,07 % |
| CPU en reposo, PDF | 0,1–0,13 % | 0,1–0,13 % |
| Three.js (2000 nudos instanciados) | 60 fps | 60 fps |

Las páginas sin diagramas no cambian: sus diferencias están dentro del ruido entre rondas.

2026-10-18, #38 (buscar en la página con Ctrl+F), mismas condiciones. Tres rondas alternadas con el binario de `main` (`667daff`). Ninguna medida cambia: las dos primeras rondas dieron +10–18 MB con `ondas.md` y el PDF, y la tercera, −1–2 MB; la nota mínima y el HTML suelto se movieron en sentido contrario. Ese día, las dos versiones daban en las mismas páginas tiempos por encima de la tabla de arriba (`ondas.md`, 419–467 ms): no compares estas cifras con ella. Coste: el binario pasa de 5 209 632 a 5 243 840 bytes (+34 KB) y el paquete de CodeMirror, +6,6 KB con brotli (ADR 0028).

### En Hyprland

2026-10-17, commit `ff0322c` (#25, #37 y #27: visor como app predeterminada, PDF en el panel y visor completo de PDF.js con anotaciones), enchufado, perfil `performance`. Una ronda alternada con la referencia anterior (`5636bd8`) en la misma ejecución: valor de esa ronda. El binario de `5636bd8` abre el PDF en el visor de PDF.js que trae WebKit, sin anotaciones. Las filas sin columna «Antes» vienen de medidas anteriores y no se repitieron.

| Medida | Valor | Antes (`5636bd8`) |
|---|---|---|
| Arranque en frío, nota mínima (`minima.md`) | 196 ms | 208 ms |
| Arranque en frío, `estudio/física/ondas.md` (Mermaid, KaTeX, código) | 637 ms | 634 ms |
| Arranque en frío, HTML suelto | 187 ms | 178 ms |
| Arranque en frío, PDF de 2 páginas (`ondas-apuntes.pdf`) | 240 ms | 233 ms (visor de WebKit) |
| Cambio de página, dentro de la app (con la CLI) | 7,4 ms; 34 ms | 9,1 ms; 36 ms |
| Pestaña nueva, dentro de la app (con la CLI) | 134 ms; 161 ms | 128 ms; 152 ms |
| Memoria en reposo, nota mínima | 211 MB | 217 MB |
| Memoria en reposo, `ondas.md` | 326 MB | 325 MB |
| Memoria en reposo, HTML suelto | 200 MB | 199 MB |
| Memoria en reposo, PDF | 304 MB | 300 MB (visor de WebKit) |
| Memoria con el editor abierto (nota mínima) | 230 MB | 230 MB |
| CPU en reposo, nota mínima u `ondas.md` | 0,03 % | 0,03–0,07 % |
| CPU en reposo, PDF | 0,17 % | 0,2 % (visor de WebKit) |
| Cada página viva extra (pestañas o vista dividida, máximo 3) | +60–65 MB con una nota, ~+90 MB con Three.js (2026-10-07) | |
| Un dibujo de Excalidraw abierto, frente a una nota | ~+45 MB en lectura; en edición, 240 MB frente a 207 MB de una nota (ADR 0036) | |
| Un dibujo con fórmulas LaTeX, frente a uno sin ellas | ~225 ms y ~170 KB la primera vez, ~70 ms al recargar; +2–5 MB (3 medidas, poco fiable) | |
| Three.js (2000 nudos instanciados) | 60 fps | 60 fps |
| Binario (`cargo build --release`, con `strip`) | 6 179 320 bytes | 5 373 992 bytes |

Las diferencias entre los dos binarios están dentro del ruido. El visor de PDF de visor cuesta lo mismo que el que trae WebKit (+4 MB, +7 ms) y, frente a una nota mínima de la misma ronda, ~+90 MB y ~+45 ms. De los 805 KB del binario, ~744 KB son PDF.js (ADR 0037); el resto, el i18n (#34), #36 y #25.

La deriva se vio dentro de la misma tarde: una ronda solo con `ff0322c`, diez minutos antes, dio 176 MB con la nota mínima, 277 MB con `ondas.md` y 258 MB con el PDF, ~35–45 MB menos que en la ronda alternada. En esa ronda, la pestaña nueva dio una vez 512 ms; repetida tres veces con la misma secuencia, 126–128 ms.

**Coste conocido, aceptado: +11,4 MB con un HTML suelto.** Sale igual en todas las repeticiones (130 → 141,5 MB) y no aparece con notas de la bóveda. Lo causan los tres botones de navegación visibles en la fila de pestañas (#22, `b9cc9c9`): el commit anterior da 130 MB, y el mismo commit con los botones creados pero sin colocar en la fila, también. No depende del icono (con botones de texto sube igual), del tooltip ni de que estén desactivados, y no carga glycin. Crece el *heap* del proceso principal (+16,7 MB) y bajan los búferes `memfd:gdk-wayland` (−5,5 MB). No es un búfer extra por redibujar: pasar el ratón por la fila no cambia la cifra de antes ni la de después. Con una bóveda, el panel lateral ya paga ese coste y los botones no añaden nada. La causa exacta en GTK3 sigue sin identificar.

En la referencia anterior (`5e67e74`, tres rondas con `6399c0b`), las dos primeras rondas daban más memoria con la nota mínima (+3–9 MB) y más tiempo de pestaña nueva (+4–12 ms); la tercera dio menos memoria con el binario nuevo y acercó la pestaña nueva (141,5 frente a 145,1 ms). Los rangos se solapan: es ruido, aunque la pestaña nueva conviene vigilarla en la próxima medida. Los 48 KB del binario son el botón de actualizar y `visor update` (~17 KB), las imágenes pegadas (~17 KB), las fórmulas (~6 KB) y el resto de cambios.

La memoria bajó ~80 MB respecto de la referencia anterior (217 MB con la nota mínima en `02727b2`) sin que cambiara el código: el binario de `02727b2`, medido el mismo día con el mismo método, da ~131 MB. La comparativa del README dio 215 MB esa tarde y la bajada apareció horas después, sin reiniciar. Se descartaron la consulta de actualizaciones, la pantalla, `alsa-plugins` y otra instancia de visor abierta (esta última baja la medida solo 3–5 MB). La causa sigue sin identificar: compara solo rondas alternadas de una misma ejecución, nunca cifras de días distintos.

Desde `f9a0320`, el cambio de página y la pestaña nueva se miden dentro de la app; las cifras de la CLI van aparte. No se comparan con las de referencias anteriores a ese commit (74 ms y 176–185 ms incluían la CLI y una espera de 50 ms).
