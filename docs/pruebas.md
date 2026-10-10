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
tools/medir/medir.py --rondas 3 /tmp/visor-antes /tmp/visor-despues
tools/medir/medir.py --resumen ${TMPDIR:-/tmp}/visor-medir/resultados-….jsonl
```

`medir.py` aísla la instancia, copia la bóveda `tools/medir/boveda/`, alterna los binarios y mide arranque en frío, memoria (PSS del árbol de procesos, 10 s tras cargar), CPU en reposo (30 s), cambio de página, pestaña nueva y editor. Three.js escribe sus fps en la consola cada segundo (`visor console`).

- Cada ronda dura ~2 min por binario y abre ventanas en el escritorio activo: no uses el equipo mientras tanto. 3 rondas para comprobar un cambio; 5 para una referencia o si una cifra se sale del ruido.
- La ventana debe estar visible en el escritorio activo: WebKit frena las páginas ocultas y falsea la CPU. El resumen descarta esas rondas. Mover el ratón sobre la ventana la redibuja y sube la CPU a ~2 %.
- Memoria en PSS, no RSS (el RSS cuenta doble).
- La página manda en la memoria: una nota con Mermaid y KaTeX ocupa ~100 MB más que una de un párrafo con el mismo binario. Compara con la misma página.
- El ruido supera a menudo el efecto: con diferencias de pocos ms, compara solo rondas alternadas. El arranque con `ondas.md` es bimodal con cualquier binario (~670 y ~725 ms).
- El perfil de energía manda: con el mismo binario, el arranque en frío va de ~220 ms (`performance`) a ~310 ms (`balanced`, enchufado), ~800 ms (batería) y ~1,1 s (`power-saver`). Compara solo en las mismas condiciones (`powerprofilesctl get`, enchufado o no).

## Referencia

2026-10-10, commit `02727b2` (fin de la revisión de arquitectura y de la revisión independiente), enchufado, perfil `performance`. Mediana (mínimo–máximo) de 3 rondas alternadas con `f2bec3e`, el commit anterior a la revisión.

| Medida | Valor | Antes (`f2bec3e`) |
|---|---|---|
| Arranque en frío, nota mínima (`minima.md`) | 221 ms (219–224) | 222 ms (221–225) |
| Arranque en frío, `estudio/física/ondas.md` (Mermaid, KaTeX, código) | 721 ms (717–722) | 722 ms (674–725) |
| Arranque en frío, HTML suelto | 219 ms (217–221) | 221 ms (219–226) |
| Cambio de página | 74 ms (74–78) | 75 ms (75–80) |
| Pestaña nueva (`visor open --tab`) | 176–185 ms | 174–178 ms |
| Memoria en reposo, nota mínima | 217 MB (217–219) | 221 MB (220–222) |
| Memoria en reposo, `ondas.md` | 324 MB (324) | 322 MB (321–326) |
| Memoria en reposo, HTML suelto | 186 MB | 186 MB |
| Memoria con el editor abierto (nota mínima) | 236–239 MB | 236–239 MB |
| CPU en reposo, nota mínima u `ondas.md` | 0–0,03 % | 0,03–0,07 % |
| Cada página viva extra (pestañas o vista dividida, máximo 3) | +60–65 MB con una nota, ~+90 MB con Three.js (2026-10-07) | |
| Three.js (2000 nudos instanciados) | 60 fps | 60 fps |
| Binario (`cargo build --release`, con `strip`) | 4 263 808 bytes | 4 221 096 bytes |

Dos cifras saltan entre ejecuciones con cualquier binario: la memoria con el editor (~236 o ~239 MB) y la pestaña nueva (una ejecución dio 182–188 ms; repetida con 4 rondas, 176–179 frente a 174–176 de `f2bec3e`). La memoria absoluta varía hasta 15 MB entre ejecuciones del mismo día con el mismo binario. Compara solo rondas alternadas de una misma ejecución.
