#!/bin/sh
# Builds the mockups and screenshots them with WebKitGTK inside a headless sway of its own,
# so nothing opens on the real desktop. Usage: ./capturar.sh [a-normal b-claro ...]
set -eu

AQUI=$(cd "$(dirname "$0")" && pwd)
TMP=$(mktemp -d)
UNIDAD=maquetas-sway-$$

cd "$AQUI"
npx vite build >/dev/null
node empaquetar.mjs >/dev/null

cat > "$TMP/sway.conf" <<EOF
output HEADLESS-1 resolution 1920x1080
xwayland disable
default_border none
for_window [title="maquetas"] floating enable, resize set 1400 900, move position 0 0
exec sh -c 'echo "\$WAYLAND_DISPLAY" > $TMP/display'
EOF

systemd-run --user --quiet --collect --unit="$UNIDAD" \
  -E WLR_BACKENDS=headless -E WLR_LIBINPUT_NO_DEVICES=1 \
  -E SWAYSOCK="$TMP/sway.sock" -E XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" \
  sway -c "$TMP/sway.conf"
trap 'systemctl --user stop "$UNIDAD" 2>/dev/null || true; rm -rf "$TMP"' EXIT

ESPERA=0
until test -s "$TMP/display" || [ "$ESPERA" -ge 10 ]; do sleep 0.5; ESPERA=$((ESPERA + 1)); done

WAYLAND_DISPLAY=$(cat "$TMP/display") SWAYSOCK="$TMP/sway.sock" GDK_BACKEND=wayland \
  timeout 150 /usr/bin/python3 capturar.py "$AQUI/dist/maqueta.html" "$AQUI/capturas" "$@"
