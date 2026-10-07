# Probar y medir

## Instancia aislada

El CLI habla con la instancia que escucha en el socket. Si el usuario tiene visor abierto (`target/release/visor`, lanzado con `cargo run --release`), cualquier `visor open` sin aislar abre páginas en su ventana y escribe en su `~/.config/visor`. Prueba siempre así:

```sh
export VISOR_SOCKET=$XDG_RUNTIME_DIR/visor-prueba.sock   # ruta corta: los sockets Unix admiten ~108 caracteres
export XDG_CONFIG_HOME=$TMP/cfg XDG_STATE_HOME=$TMP/state XDG_DATA_HOME=$TMP/data XDG_CACHE_HOME=$TMP/cache
target/release/visor & echo $! > $TMP/pid      # ciérrala con kill $(cat $TMP/pid)
```

Cierra solo por PID. Un `pkill -f` con un patrón que aparezca en tu propia orden mata tu shell, y uno con la ruta relativa alcanza la instancia del usuario.

## Interactuar sin ratón

- Teclas reales a una ventana (Hyprland usa configuración en Lua):
  `hyprctl dispatch 'hl.dsp.send_key_state({ mods = "CTRL", key = "E", state = "down", window = "address:0x…" })'` y lo mismo con `state = "up"`. La dirección sale de `hyprctl clients -j` filtrando por `pid`.
- Captura: `grim -g "x,y anchoxalto"` con la geometría de `hyprctl clients -j`.
- Para gestos que no se pueden enviar (selección de texto, clics), escribe una página de prueba en la bóveda que los reproduzca por script (`dispatchEvent`, `.click()` dentro de `document.querySelector("visor-capa").shadowRoot`) y lee el resultado con `visor console`.
- URL base del servidor (lleva una clave que cambia en cada arranque): una página con `visor.send({ href: location.href })` y `visor events --limit 1`.

## Medir

- **Arranque en frío**: tiempo de `visor open <archivo>` con la app cerrada; ya espera a que la página cargue.
- **Memoria**: suma de `Pss` (`/proc/<pid>/smaps_rollup`) del proceso y de sus hijos y nietos (WebKitNetworkProcess, WebKitWebProcess, bwrap). El RSS cuenta doble.
- Repite cada medida al menos cinco veces; con diferencias de pocos milisegundos, haz rondas alternadas. El ruido supera a menudo el efecto.
- **El perfil de energía manda**: con el mismo binario, el arranque en frío va de ~280 ms (`performance`) a ~310 ms (`balanced`, enchufado), ~800 ms (batería) y ~1,1 s (`power-saver`). Anota `powerprofilesctl get` y si el equipo está enchufado; compara solo en las mismas condiciones.

## Referencia (2026-10-07, enchufado, `performance`)

| Medida | Valor |
|---|---|
| Arranque en frío hasta página cargada | 267–283 ms |
| Cambio de página | ~80 ms |
| Pestaña nueva (`visor open --tab`) | ~130 ms |
| Memoria en reposo, una página | ~307–313 MB |
| Memoria con el editor abierto (Ctrl+E) | ~334–338 MB |
| Cada página viva extra (pestañas o vista dividida, máximo 3) | +60–65 MB con una nota, ~+90 MB con Three.js |
| Three.js (2000 nudos instanciados) | 59–60 fps |
| Binario | 3,94 MiB |
