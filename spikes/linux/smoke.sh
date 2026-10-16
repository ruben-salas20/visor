#!/usr/bin/env bash
# Prueba de arranque de visor en una distro: ¿responde `visor status` bajo xvfb?
# Uso: smoke.sh <distro> <binario> <resultado.json>
# Escribe el resultado en JSON y en la salida: "funciona" o "falla (motivo)".
# Sale con 0 si funciona y con 1 si falla, para que el paso quede marcado.
#
# Uso interno: smoke.sh --app <binario> <dir> <veredicto>  (lo llama xvfb-run)
set -uo pipefail

here=$(cd "$(dirname "$0")" && pwd)

# Arranca visor aislado y consulta `status` hasta 20 s. Corre dentro de xvfb-run.
run_app() {
  local bin=$1 tmp=$2 verdict=$3
  local pid deadline out err

  verdict_line() { printf '%s\n%s\n' "$1" "$2" > "$verdict"; }

  # Todo el estado en $tmp: no toca la configuración ni el socket de nadie.
  export VISOR_SOCKET="$tmp/v.sock"
  export XDG_RUNTIME_DIR="$tmp/run"
  export XDG_CONFIG_HOME="$tmp/cfg" XDG_STATE_HOME="$tmp/state"
  export XDG_DATA_HOME="$tmp/data" XDG_CACHE_HOME="$tmp/cache"
  export VISOR_NO_UPDATE=1 GDK_BACKEND=x11
  mkdir -p -m 700 "$XDG_RUNTIME_DIR"
  mkdir -p "$tmp/boveda"
  printf '# Prueba\n\nNota mínima para el arranque.\n' > "$tmp/boveda/nota.md"

  "$bin" >"$tmp/app.out" 2>"$tmp/app.err" &
  pid=$!
  deadline=$((SECONDS + 20))
  while (( SECONDS < deadline )); do
    if out=$("$bin" status 2>/dev/null) && grep -q '"ok": true' <<<"$out"; then
      # Abre la nota para cargar una bóveda; el resultado de la prueba es que status responda.
      "$bin" open "$tmp/boveda/nota.md" >/dev/null 2>&1
      kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
      verdict_line ok ""
      return 0
    fi
    if ! kill -0 "$pid" 2>/dev/null; then
      wait "$pid" 2>/dev/null; local code=$?
      err=$(tail -n 3 "$tmp/app.err" | tr '\n' ' ')
      verdict_line falla "se cae al arrancar (código $code): ${err:-sin salida en stderr}"
      return 0
    fi
    sleep 0.5
  done
  err=$(tail -n 3 "$tmp/app.err" | tr '\n' ' ')
  kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
  verdict_line falla "no responde a status en 20 s${err:+ (stderr: $err)}"
}

if [ "${1:-}" = "--app" ]; then
  run_app "$2" "$3" "$4"
  exit 0
fi

distro=${1:?uso: smoke.sh <distro> <binario> <resultado.json>}
src=${2:?falta el binario}
result=${3:?falta el archivo de resultado}
tmp=$(mktemp -d)

# Escapa texto para meterlo en una cadena JSON.
esc() { printf '%s' "$1" | tr '\n\r\t' '   ' | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g'; }

# Escribe el JSON, imprime el veredicto y sale.
finish() {
  local resultado=$1 motivo=$2
  mkdir -p "$(dirname "$result")"
  printf '{\n  "distro": "%s",\n  "glibc": "%s",\n  "webkit": "%s",\n  "resultado": "%s",\n  "motivo": "%s"\n}\n' \
    "$(esc "$distro")" "$(esc "$glibc_sys")" "$(esc "$webkit")" "$resultado" "$(esc "$motivo")" > "$result"
  if [ "$resultado" = funciona ]; then
    echo "RESULTADO $distro: funciona"
    exit 0
  fi
  echo "RESULTADO $distro: falla (${motivo})"
  echo "::warning title=$distro::falla: $motivo"
  exit 1
}

# glibc del sistema; vacío si la distro usa musl (Alpine).
glibc_sys=$(getconf GNU_LIBC_VERSION 2>/dev/null | awk '{print $2}')
webkit="no instalado"

inst=$("$here/install-runtime.sh" "$distro" 2>"$tmp/install.err")
rc=$?
tail -n 5 "$tmp/install.err" >&2
webkit=$(grep '^WEBKIT=' <<<"$inst" | tail -n1 | cut -d= -f2-)
if [ "$rc" -ne 0 ]; then
  finish falla "$(grep '^MOTIVO=' <<<"$inst" | tail -n1 | cut -d= -f2-)"
fi
webkit=${webkit:-desconocida}

# El artefacto llega sin bit de ejecución (upload-artifact no lo conserva).
bin="$tmp/visor"
cp "$src" "$bin" && chmod +x "$bin" || finish falla "no se pudo copiar el binario"

need=$("$here/glibc-report.sh" "$bin" --max)
if [ "$need" != ninguna ]; then
  if [ -z "$glibc_sys" ]; then
    finish falla "sin glibc (musl); el binario exige glibc $need"
  fi
  newest=$(printf '%s\n%s\n' "$need" "$glibc_sys" | sort -V | tail -n1)
  if [ "$newest" != "$glibc_sys" ]; then
    finish falla "glibc $glibc_sys del sistema; el binario exige $need"
  fi
fi

missing=$(ldd "$bin" 2>&1 | grep -E 'not found|No such file' | tr -s ' \t' ' ' | tr '\n' ';')
if [ -n "$missing" ]; then
  finish falla "faltan bibliotecas: $missing"
fi

verdict="$tmp/veredicto"
rm -f "$verdict"
xvfb-run -a bash "$here/smoke.sh" --app "$bin" "$tmp" "$verdict" >"$tmp/xvfb.log" 2>&1
if [ ! -s "$verdict" ]; then
  finish falla "xvfb-run no dejó veredicto: $(tail -n 2 "$tmp/xvfb.log" | tr '\n' ' ')"
fi
estado=$(sed -n 1p "$verdict")
motivo=$(sed -n 2p "$verdict")
if [ "$estado" = ok ]; then
  finish funciona ""
fi
finish falla "$motivo"
