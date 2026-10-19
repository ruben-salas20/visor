#!/bin/sh
# Runs the prototype's test in its own headless sway. Never opens windows on
# the real desktop: sway and the program use their own Wayland socket and SWAYSOCK.
# Input is real: a wlroots virtual pointer (target/release/puntero) and wtype.
set -eu

ORIGEN=$(cd "$(dirname "$0")/.." && pwd)
TMP=$(mktemp -d)
UNIDAD=menus-sway-$$

cat > "$TMP/sway.conf" <<EOF
output HEADLESS-1 resolution 1920x1080 scale ${ESCALA:-1}
xwayland disable
default_border none
default_floating_border none
focus_follows_mouse no
for_window [title="menus-encima"] floating enable, move position 0 0
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
export ESCALA="${ESCALA:-1}"

cd "$ORIGEN"
estado=0
timeout 170 ./target/release/menus-encima --prueba "$TMP/informe.json" "$@" || estado=$?
grim "$TMP/final.png" 2>/dev/null || true
systemctl --user stop "$UNIDAD" || true
echo "salida: $estado; informe: $TMP/informe.json; captura: $TMP/final.png"
