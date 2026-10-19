#!/bin/sh
# Lanza las pruebas de U1 en un sway sin pantalla (headless) propio.
# Nunca abre ventanas en el escritorio real: sway y el programa usan su propio
# socket Wayland y su propio SWAYSOCK, dentro de un directorio temporal.
set -eu

ORIGEN=$(cd "$(dirname "$0")/.." && pwd)
TMP=$(mktemp -d)
UNIDAD=u1-sway

cat > "$TMP/sway.conf" <<EOF
output HEADLESS-1 resolution 1920x1080
xwayland disable
for_window [title="U1 marco HTML"] floating enable, move position 0 0
exec sh -c 'echo "\$WAYLAND_DISPLAY" > $TMP/display'
EOF

systemd-run --user --quiet --collect --unit="$UNIDAD" \
  -E WLR_BACKENDS=headless -E WLR_LIBINPUT_NO_DEVICES=1 \
  -E SWAYSOCK="$TMP/sway.sock" -E XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" \
  sway -c "$TMP/sway.conf"

sleep 2
if ! test -s "$TMP/display"; then
  echo "sway no arrancó: falta $TMP/display" >&2
  systemctl --user stop "$UNIDAD" || true
  exit 1
fi

WAYLAND_DISPLAY=$(cat "$TMP/display")
export WAYLAND_DISPLAY
export SWAYSOCK="$TMP/sway.sock"
unset DISPLAY

cd "$ORIGEN"
./target/release/u1-marco-html --prueba "$TMP/informe.json" &
PID=$!
echo "PID del programa: $PID (directorio temporal: $TMP)"

ESPERA=0
while kill -0 "$PID" 2>/dev/null && [ "$ESPERA" -lt 120 ]; do
  sleep 1
  ESPERA=$((ESPERA + 1))
done
kill "$PID" 2>/dev/null || true
wait "$PID" 2>/dev/null || true

systemctl --user stop "$UNIDAD" || true
echo "Informe: $TMP/informe.json"
cat "$TMP/informe.json" 2>/dev/null || echo "sin informe"
