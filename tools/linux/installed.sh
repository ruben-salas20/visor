#!/usr/bin/env bash
# Smoke test of an installed visor (.deb or Flatpak), for .github/workflows/release.yml.
# Runs inside xvfb-run and dbus-run-session. Exits non-zero on the first failure.
# Usage: installed.sh <deb|flatpak>
set -uo pipefail

mode=${1:?usage: installed.sh <deb|flatpak>}
ID=io.github.ruben_salas20.visor
tmp=$(mktemp -d)
vault="$HOME/visor-prueba"
mkdir -p "$vault"
printf '# Nota de prueba\n\nTexto.\n' > "$vault/nota.md"

if [ "$mode" = deb ]; then
  V=(/usr/bin/visor)
  # Its own socket and folders: touches nobody's configuration.
  export VISOR_SOCKET="$tmp/run/v.sock" XDG_RUNTIME_DIR="$tmp/run"
  export XDG_CONFIG_HOME="$tmp/cfg" XDG_STATE_HOME="$tmp/state"
  export XDG_DATA_HOME="$tmp/data" XDG_CACHE_HOME="$tmp/cache"
  mkdir -p -m 700 "$XDG_RUNTIME_DIR"
  entries=("$tmp/data/applications/visor.desktop")
  [ -f /usr/share/applications/visor.desktop ] || { echo "FALLO: el .deb no instala visor.desktop"; exit 1; }
  [ -f /usr/share/icons/hicolor/scalable/apps/visor.svg ] || { echo "FALLO: el .deb no instala el icono"; exit 1; }
else
  V=(flatpak run "$ID")
  entries=("$HOME/.var/app/$ID/data/applications/visor.desktop" "$HOME/.local/share/applications/visor.desktop")
fi
export GDK_BACKEND=x11

fail() { echo "FALLO: $*"; [ -f "$tmp/app.err" ] && tail -n 20 "$tmp/app.err"; exit 1; }

"${V[@]}" >"$tmp/app.out" 2>"$tmp/app.err" &
pid=$!
up=no
for _ in $(seq 120); do
  if "${V[@]}" status 2>/dev/null | grep -q '"ok": true'; then up=yes; break; fi
  sleep 0.5
done
[ "$up" = yes ] || fail "status no responde en 60 s"
echo "ok: arranca y status responde"

out=$("${V[@]}" open "$vault/nota.md" 2>&1)
grep -q '"ok": true' <<<"$out" || fail "open: $out"
echo "ok: abre una nota"

for f in "${entries[@]}"; do
  [ -e "$f" ] && fail "escribió su propia entrada de escritorio: $f"
done
echo "ok: no escribe su entrada de escritorio"

out=$(env -u VISOR_NO_UPDATE "${V[@]}" update 2>&1)
grep -qE 'actualiza su paquete|updated by its package' <<<"$out" || fail "update: $out"
echo "ok: update dice que lo actualiza el paquete"

if [ "$mode" = flatpak ]; then flatpak kill "$ID" >/dev/null 2>&1; else kill "$pid" 2>/dev/null; fi
wait "$pid" 2>/dev/null
rm -rf "$tmp"
echo "pruebas de $mode: todo bien"
