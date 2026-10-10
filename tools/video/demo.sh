#!/usr/bin/env bash
# Records a real visor session for the README GIF: the person underlines a phrase and asks,
# the agent (a foot window below) waits for the event, answers in the note and opens a
# Markdown note beside the page.
#
#   tools/video/demo.sh [es|en|todos]        default: todos
#
# Needs Hyprland, grim, foot, jq, ffmpeg and target/release/visor. Each run uses its own
# instance (VISOR_SOCKET and XDG_* under $DEMO_TMP, or a new temporary folder), so a visor
# the user has open is never touched. Floats both windows over the active workspace for
# ~25 s: do not use the computer meanwhile. Output: docs/img/demo-captura.<lang>.gif.
# The person's gestures are played by tools/video/demo-*/<folder>/gestos.js.
set -euo pipefail

RAIZ=$(cd "$(dirname "$0")/../.." && pwd)
VISOR=$RAIZ/target/release/visor
TRABAJO=${DEMO_TMP:-$(mktemp -d)}
FPS=${FPS:-15}
# Recorded region in logical pixels (16:9): visor on top, the terminal below.
X=192 Y=169 ANCHO=1152 ALTO_VISOR=480 ALTO_TERM=168
ALTO=$((ALTO_VISOR + ALTO_TERM))

case ${1:-todos} in
  todos) IDIOMAS=(es en) ;;
  es | en) IDIOMAS=("$1") ;;
  *) echo "uso: $0 [es|en|todos]" >&2; exit 2 ;;
esac

PIDS=()
cleanup() { for p in "${PIDS[@]}"; do kill "$p" 2>/dev/null || true; done; }
trap cleanup EXIT

window_of() { # Waits for the window of a PID and prints its address.
  local a
  for _ in $(seq 100); do
    a=$(hyprctl clients -j | jq -r --argjson p "$1" '.[] | select(.pid == $p) | .address' | head -1)
    [ -n "$a" ] && { echo "$a"; return; }
    sleep 0.1
  done
  echo "sin ventana para el PID $1" >&2
  return 1
}

place() { # address x y width height
  local w="window = \"address:$1\""
  hyprctl -q dispatch "hl.dsp.window.fullscreen({ mode = \"fullscreen\", action = \"unset\", $w })"
  hyprctl -q dispatch "hl.dsp.window.float({ action = \"enable\", $w })"
  hyprctl -q dispatch "hl.dsp.window.resize({ x = $4, y = $5, $w })"
  hyprctl -q dispatch "hl.dsp.window.move({ x = $2, y = $3, $w })"
  hyprctl -q dispatch "hl.dsp.window.tag({ tag = \"+visordemo\", $w })"
  hyprctl -q dispatch "hl.dsp.window.tag({ tag = \"-default-opacity\", $w })"
}

# Square, borderless and opaque, so the frame shows nothing of what lies behind.
# Runtime rule: it lasts until Hyprland reloads its configuration and only matches the tag.
hyprctl -q eval 'hl.window_rule({ match = { tag = "visordemo" }, rounding = 0, border_size = 0, no_shadow = true, opacity = "1 override 1 override" })'

grabar() {
  local lang=$1 W=$TRABAJO/$1
  local B INICIO PAGINA NOTA RESPUESTA
  case $lang in
    es)
      B=$W/estudio INICIO=física/sonido.md PAGINA=física/ondas.html NOTA=física/energía.md
      RESPUESTA="No: la energía va con el cuadrado de la amplitud. En fase, doble amplitud y cuatro veces la energía; en oposición, cero. Te abro una nota."
      ;;
    en)
      B=$W/study INICIO=physics/sound.md PAGINA=physics/waves.html NOTA=physics/energy.md
      RESPUESTA="No: energy goes with the square of the amplitude. In phase, twice the amplitude and four times the energy; in opposition, zero. Opening a note for you."
      ;;
  esac

  rm -rf "$W"
  mkdir -p "$W"
  cp -r "$RAIZ/tools/video/demo-$lang" "$B"
  touch "$B/$(dirname "$PAGINA")/gestos.on"

  export VISOR_SOCKET=$XDG_RUNTIME_DIR/visor-demo.sock VISOR_NO_UPDATE=1
  export XDG_CONFIG_HOME=$W/cfg XDG_STATE_HOME=$W/state XDG_DATA_HOME=$W/data XDG_CACHE_HOME=$W/cache
  if "$VISOR" status >/dev/null 2>&1; then
    echo "Ya responde una instancia en $VISOR_SOCKET; ciérrala antes." >&2
    exit 1
  fi
  mkdir -p "$XDG_CONFIG_HOME/visor"
  jq -n --arg b "$B" '{vaults: [$b], current: $b, theme: "oscuro", sidebar_width: 200}' >"$XDG_CONFIG_HOME/visor/config.json"
  cat >"$W/foot.ini" <<'EOF'
[main]
font=JetBrainsMono Nerd Font:size=11
pad=14x10
[colors-dark]
background=0d0d0d
foreground=d0d0d0
EOF

  cd "$B"
  "$VISOR" >"$W/visor.log" 2>&1 &
  local visor_pid=$!
  PIDS+=("$visor_pid")
  local va
  va=$(window_of "$visor_pid")
  "$VISOR" open "$INICIO" >/dev/null
  place "$va" $X $Y $ANCHO $ALTO_VISOR

  PAGINA=$PAGINA NOTA=$NOTA RESPUESTA=$RESPUESTA \
    foot -c "$W/foot.ini" -a visor-demo-term -T agente \
    bash "$RAIZ/tools/video/agente.sh" "$VISOR" "$W/go" "$W/done" 2>/dev/null &
  local foot_pid=$!
  PIDS+=("$foot_pid")
  place "$(window_of "$foot_pid")" $X $((Y + ALTO_VISOR)) $ANCHO $ALTO_TERM
  hyprctl -q dispatch "hl.dsp.focus({ window = \"address:$va\" })"
  sleep 2

  python3 "$RAIZ/tools/video/capturar.py" "$X,$Y ${ANCHO}x$ALTO" "$W/frames" --fps "$FPS" &
  local rec_pid=$!
  PIDS+=("$rec_pid")
  sleep 0.5
  touch "$W/go"
  for _ in $(seq 1200); do [ -e "$W/done" ] && break; sleep 0.1; done
  sleep 4.5 # time to read the note
  kill -TERM "$rec_pid"
  wait "$rec_pid"
  kill "$foot_pid" "$visor_pid" 2>/dev/null || true
  cd "$RAIZ"

  local out=$RAIZ/docs/img/demo-captura.$lang.gif
  ffmpeg -v error -y -framerate "$FPS" -i "$W/frames/seq/%05d.ppm" -vf \
    "scale=960:540:flags=lanczos,split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle" \
    "$out"
  echo "$out: $(du -h "$out" | cut -f1), fotogramas en $W/frames/seq"
}

for lang in "${IDIOMAS[@]}"; do grabar "$lang"; done
