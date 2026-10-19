#!/usr/bin/env bash
# Pruebas a–f del experimento L5 sobre una copia de visor ya instalada.
# Uso: pruebas.sh <deb|flatpak> <carpeta-de-resultados>
# Corre dentro de xvfb-run y dbus-run-session. Escribe <carpeta>/resultado.tsv
# con líneas "clave<TAB>valor". Los errores de visor se guardan tal cual.
set -uo pipefail

modo=${1:?uso: pruebas.sh <deb|flatpak> <carpeta>}
out=${2:?falta la carpeta de resultados}
mkdir -p "$out"
tsv="$out/resultado.tsv"
: > "$tsv"

ID=io.github.ruben_salas20.visor
emit() { printf '%s\t%s\n' "$1" "$2" >> "$tsv"; }

tmp=$(mktemp -d)
boveda="$HOME/l5-boveda"
mkdir -p "$boveda"
printf '# Nota L5\n\nTexto de prueba para el experimento.\n' > "$boveda/nota.md"

if [ "$modo" = deb ]; then
  # Estado aislado: no toca la configuración ni el socket de nadie.
  V=(/usr/bin/visor)
  export VISOR_SOCKET="$tmp/run/v.sock" XDG_RUNTIME_DIR="$tmp/run"
  export XDG_CONFIG_HOME="$tmp/cfg" XDG_STATE_HOME="$tmp/state"
  export XDG_DATA_HOME="$tmp/data" XDG_CACHE_HOME="$tmp/cache" GDK_BACKEND=x11
  mkdir -p -m 700 "$XDG_RUNTIME_DIR"
  escritorio=("$tmp/data/applications/visor.desktop")
else
  # Flatpak: el socket y los datos están dentro de la sandbox.
  V=(flatpak run "$ID")
  escritorio=("$HOME/.var/app/$ID/data/applications/visor.desktop" "$HOME/.local/share/applications/visor.desktop")
fi

# Mide la mediana de 10 llamadas a `status`, en milisegundos.
medida() {
  local ts=() s e i
  for i in $(seq 10); do
    s=$(date +%s%N)
    "${V[@]}" status >/dev/null 2>&1
    e=$(date +%s%N)
    ts+=($(( (e - s) / 1000000 )))
  done
  printf '%s\n' "${ts[@]}" | sort -n | awk 'NR==5{a=$1} NR==6{b=$1} END{printf "%.1f", (a+b)/2}'
}

# Arranque: visor abre la ventana y `status` responde. Hasta 60 s.
t0=$(date +%s%N)
"${V[@]}" >"$out/app.out" 2>"$out/app.err" &
pid=$!
arrancado=no
for _ in $(seq 120); do
  if "${V[@]}" status 2>/dev/null | grep -q '"ok": true'; then
    arrancado=si
    break
  fi
  sleep 0.5
done
t1=$(date +%s%N)
if [ "$arrancado" = si ]; then
  emit arranque "funciona: $(( (t1 - t0) / 1000000 )) ms hasta status ok"
else
  emit arranque "falla: status no responde en 60 s; stderr: $(tail -n 3 "$out/app.err" | tr '\n' ' ')"
fi

# a) Abre una nota de una bóveda dentro de $HOME.
a=$("${V[@]}" open "$boveda/nota.md" 2>&1)
if grep -q '"ok": true' <<<"$a"; then
  emit a_boveda_en_home "funciona"
else
  emit a_boveda_en_home "falla: $(tr '\n' ' ' <<<"$a")"
fi

# b) o c) Tiempo de la CLI desde fuera, mediana de 10.
if [ "$modo" = flatpak ]; then
  emit b_cli_flatpak_mediana_ms "$(medida)"
else
  emit c_cli_nativo_mediana_ms "$(medida)"
fi

# d) Servidor MCP por entrada estándar: initialize y status.
mcp=$(printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"l5","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"status","arguments":{}}}' \
  | timeout 60 "${V[@]}" mcp 2>&1)
linea1=$(grep '"id":1' <<<"$mcp")
linea2=$(grep '"id":2' <<<"$mcp")
if [ -n "$linea1" ] && grep -q 'protocolVersion' <<<"$linea1" && [ -n "$linea2" ] && ! grep -q '"isError":true' <<<"$linea2"; then
  emit d_mcp "funciona"
else
  emit d_mcp "falla: $(tr '\n' ' ' <<<"$mcp" | cut -c1-300)"
fi

# e) No escribe el archivo .desktop y avisa de que la actualiza el paquete.
escrito=no
for f in "${escritorio[@]}"; do
  [ -e "$f" ] && escrito="$f"
done
if [ "$escrito" = no ]; then
  emit e_sin_archivo_escritorio "funciona"
else
  emit e_sin_archivo_escritorio "falla: se escribió $escrito"
fi
upd=$(env -u VISOR_NO_UPDATE "${V[@]}" update 2>&1)
if grep -q 'actualiza su paquete' <<<"$upd"; then
  emit e_aviso_actualizacion "funciona"
else
  emit e_aviso_actualizacion "falla: $(tr '\n' ' ' <<<"$upd" | cut -c1-300)"
fi

# f) Carpeta en /tmp: se registra el mensaje exacto que devuelve visor.
mkdir -p /tmp/l5-carpeta
f=$("${V[@]}" open /tmp/l5-carpeta 2>&1)
emit f_carpeta_tmp "$(tr '\n' ' ' <<<"$f" | cut -c1-300)"

# Parada limpia.
if [ "$modo" = flatpak ]; then
  flatpak kill "$ID" >/dev/null 2>&1
else
  kill "$pid" 2>/dev/null
fi
wait "$pid" 2>/dev/null
rm -rf "$tmp"
echo "pruebas $modo terminadas: $tsv"
