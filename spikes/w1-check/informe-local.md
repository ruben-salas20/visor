# W1: cuánto de visor compila en Windows sin la interfaz (ejecución local)

Experimento de `docs/plan-multiplataforma.md`. Se quitó la interfaz gráfica de una copia del crate
y se pasó `cargo check` para `x86_64-pc-windows-msvc` desde Linux. No se corrigió ningún error
del código de visor.

## Qué se hizo

- `spikes/w1-check/prepare.py DESTINO [--stub-seams] [--drop-unix-imports]`: copia el crate, quita
  las dependencias de interfaz (cairo-rs, gtk, rfd, tao, webkit2gtk, wry), deja los módulos `ui/`
  portables (action, address, page_calls, tabs, watch) y quita `app` de `main.rs`.
  - `--stub-seams`: en la copia, sustituye las tres costuras por rellenos con `unimplemented!()`:
    `ui::page_view::is_web`, la papelera de GTK en `vault/files.rs::trash` y
    `tao::event_loop::EventLoopProxy` en `ui/watch.rs`.
  - `--drop-unix-imports`: borra los `use std::os::unix…` de nivel superior de la copia.
- `spikes/w1-check/classify.py`: agrupa los errores de `cargo check --message-format=json` por
  archivo y categoría, y escribe Markdown.
- Control en Linux: `prepare.py --control` (conserva las dependencias y añade `page_view`).
- Ejecución: cada pasada en su copia, `cargo check --target x86_64-pc-windows-msvc`.

## Tres pasadas

Cada pasada añade un paso sobre la anterior. Las tres se ejecutan en el flujo de CI.

| Pasada | Qué quita o rellena | Errores | Categorías |
|---|---|---:|---|
| 1. base | nada (solo recorte de interfaz) | 21 | 4 interfaz, 17 `std::os::unix` |
| 2. costuras | las 3 costuras rellenas | 17 | 17 `std::os::unix` |
| 3. sin imports unix | además, los `use std::os::unix` de nivel superior | 18 | 18 `std::os::unix` |

Controles en Linux:

| Copia | Resultado de cargo | Errores | Avisos |
|---|---|---:|---:|
| Control (con dependencias, con `page_view`) | correcto | 0 | 271 |
| Pasada 2 en Linux nativo (sin `--target`) | correcto | 0 | 264 |
| Pasada 1 en Linux nativo (sin `--target`) | fallido | 4 | - |

## Lectura por pasada

**Pasada 1.** Los 4 errores de interfaz son de resolución (E0432/E0433): rustc no llega a comprobar
tipos. Los 17 de `std::os::unix` son los que solo aparecen en Windows: 7 importaciones y 10
métodos de extensión (`dev` e `ino` de `Metadata`, `from_mode` de `Permissions`, `process_group`
de `Command`).

**Pasada 2.** Con las costuras rellenas, la copia compila en Linux sin errores, así que los rellenos
no rompen nada. En Windows desaparecen los 4 errores de interfaz y rustc ya comprueba tipos. Quedan
17 errores, todos de `std::os::unix`: las 7 importaciones y los 10 métodos. No aparece ningún
error de tipos nuevo. Esto no dice que no haya nada más: el código que usa los tipos Unix sin
importarlos no se ve hasta que se quitan las importaciones (pasada 3).

**Pasada 3.** Al quitar las importaciones, rustc deja de ocultar 8 usos de `UnixListener` y
`UnixStream` que antes no se contaban:

- `src/agent/server.rs:15`, `:25`
- `src/agent/socket.rs:31`, `:32`, `:48`, `:64`, `:69`
- `src/agent/cli.rs:37`

Los 10 métodos de extensión siguen dando error (con las líneas desplazadas una posición). El total
pasa de 17 a 18 (10 métodos y 8 tipos). Todo lo que aparece en esta pasada es `std::os::unix`;
no hay errores de tipos de otra clase detrás.

## Ubicación de los errores (pasada 1)

- `src/agent/socket.rs`: 6 (std::os::unix)
- `src/vault.rs`: 5 (std::os::unix)
- `src/agent/cli.rs`: 3 (std::os::unix)
- `src/update.rs`: 2 (std::os::unix)
- `src/agent/server.rs`: 1 (std::os::unix)
- `src/ui/address.rs`: 1 (interfaz: `crate::ui::page_view`)
- `src/ui/tabs.rs`: 1 (interfaz: `crate::ui::page_view`)
- `src/ui/watch.rs`: 1 (interfaz: `tao`)
- `src/vault/files.rs`: 1 (interfaz: `gtk`)

## Hallazgos fuera de la lista del plan

- `src/vault/files.rs::trash` usa GTK (`gtk::gio`) en código de producción. El plan supone que
  `vault/` es portable; no lo es mientras esa función use GTK. En la pasada 2 se sustituye por un
  relleno, así que aquí no se sabe qué hay que hacer con ella en Windows.
- `src/ui/watch.rs` usa `tao::event_loop::EventLoopProxy` dentro de un módulo que el plan considera
  portable. Hay que aislarlo o sustituirlo.
- Las pruebas (`#[cfg(test)]`) no se comprueban con `cargo check`. Eso incluye
  `src/vault/walk_tests.rs`, que la pasada 3 también recorta.
- "Rutas Unix" sale 0 en todas las pasadas: un error de compilación no muestra semántica de
  rutas, solo símbolos ausentes. Eso solo se verá al ejecutar.

## Límites de los rellenos

- Son aproximaciones con la misma firma de entrada, no copias del tipo real. El relleno de
  `EventLoopProxy` devuelve `Result<(), ()>`; tao devuelve `Result<(), EventLoopClosed<T>>`.
  `watch.rs` ignora ese valor, así que no cambia el resultado del check, pero no es idéntico.
- Un relleno que usa `unimplemented!()` solo prueba que los tipos cuadran; no prueba
  comportamiento.
- Los rellenos solo están en la copia generada (`/tmp/claude-1000/`); ningún archivo del repo
  cambia por ellos.

## Supuestos

- `cargo check` en Windows es el criterio de "compila"; no se ha compilado ni ejecutado nada.
- La categoría "dependencias de interfaz" incluye las costuras; no son fallos de Windows.
- El conteo viene de los mensajes JSON de rustc. Los avisos no cuentan como errores.
- `classify.py` clasifica `UnixListener` y `UnixStream` como `std::os::unix`. Antes caían en
  "otros"; esa regla se corrigió tras ver la pasada 3. La pasada 1 no cambia con la corrección.

## Pendiente

- La CI no se ha ejecutado. El flujo `.github/workflows/windows-check.yml` tiene tres pasadas.
  Su YAML se comprueba con pyyaml (carga bien, dos disparadores, seis pasos) y su bloque `run`
  se ha simulado en Linux con `RUNNER_TEMP` propio y un `cargo` de prueba que añade el destino
  Windows. La simulación confirma la lógica del script, no el entorno real de `windows-2025`.
- Riesgo sin probar: el flujo usa `$RUNNER_TEMP` (ruta de Windows) dentro de `bash` en
  `windows-2025`. Puede requerir convertir la ruta si `cd` falla. Se verá en la primera ejecución.
- Si cargo no llega a `build-finished`, `classify.py` sale con código 1 y el paso falla.
- Por decidir antes de la siguiente fase: qué hacer con la papelera de GTK en `vault/files.rs`
  y con `tao` en `ui/watch.rs`, y si los 8 usos de `UnixListener`/`UnixStream` van al mismo
  módulo de plataforma que los métodos de extensión.
