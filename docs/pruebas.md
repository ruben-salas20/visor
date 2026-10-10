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

2026-10-10, commit `8e4ae91` (fin de la revisión de arquitectura), enchufado, perfil `performance`. Mediana (mínimo–máximo) de 3 rondas alternadas con `f2bec3e`, el commit anterior a la revisión. Otras 2 rondas no cuentan: el equipo cambió de estado y el arranque se duplicó con ambos binarios.

| Medida | Valor | Antes (`f2bec3e`) |
|---|---|---|
| Arranque en frío, nota mínima (`minima.md`) | 221 ms (218–222) | 220 ms (220–227) |
| Arranque en frío, `estudio/física/ondas.md` (Mermaid, KaTeX, código) | 669 ms (665–669) | 669 ms (668–671) |
| Arranque en frío, HTML suelto | 222 ms (214–565) | 215 ms (215–218) |
| Cambio de página | 74 ms (73–75) | 76 ms (74–78) |
| Pestaña nueva (`visor open --tab`) | 175 ms (174–176) | 177 ms (176–177) |
| Memoria en reposo, nota mínima | 203 MB (202–203) | 206 MB (205–221) |
| Memoria en reposo, `ondas.md` | 317 MB (315–317) | 317 MB (315–322) |
| Memoria en reposo, HTML suelto | 186 MB | 186 MB |
| Memoria con el editor abierto (nota mínima) | 223 MB | 223 MB |
| CPU en reposo, nota mínima u `ondas.md` | 0,03 % | 0,03–0,1 % |
| Cada página viva extra (pestañas o vista dividida, máximo 3) | +60–65 MB con una nota, ~+90 MB con Three.js (2026-10-07) | |
| Three.js (2000 nudos instanciados) | 60 fps | 60 fps |
| Binario (`cargo build --release`, con `strip`) | 4 231 712 bytes | 4 221 096 bytes |

La memoria absoluta varía hasta 15 MB entre ejecuciones del mismo día con el mismo binario (el 2026-10-09, la nota mínima daba 216–220 MB). Compara solo rondas alternadas de una misma ejecución.
