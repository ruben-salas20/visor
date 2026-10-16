#!/usr/bin/env bash
# Instala el runtime de WebKitGTK 4.1 y un servidor X virtual (xvfb-run) con el
# gestor de la distro. Uso: install-runtime.sh <distro>
#   distro: ubuntu-22.04 ubuntu-24.04 debian-12 debian-13 fedora-42 fedora-43
#           arch opensuse-tumbleweed almalinux-9 alpine
# Última línea de stdout si va bien: WEBKIT=<versión instalada>.
# Si falla: MOTIVO=<texto> en stdout y código de salida 1.
set -uo pipefail

distro=${1:?uso: install-runtime.sh <distro>}
log=$(mktemp)

fail() {
  echo "MOTIVO=$1"
  exit 1
}

# Corre el gestor; si falla, traduce el error a un motivo legible.
# $1 = paquetes separados por espacio (solo para el mensaje).
run() {
  local pkgs=$1; shift
  if "$@" >"$log" 2>&1; then
    return 0
  fi
  tail -n 15 "$log" >&2
  if grep -qiE 'No match for argument|Unable to find a match|Unknown package|Unable to locate package|target not found|not found in package|has no installation candidate|no encontrado|Package .* not found' "$log"; then
    fail "paquete no existe en la distro ($pkgs)"
  fi
  fail "fallo al instalar ($pkgs); ver el log del paso"
}

case "$distro" in
  ubuntu-22.04|ubuntu-24.04|debian-12|debian-13)
    export DEBIAN_FRONTEND=noninteractive
    pkgs="libwebkit2gtk-4.1-0 xvfb xauth"
    run "$pkgs" sh -c "apt-get update && apt-get install -y --no-install-recommends $pkgs"
    webkit=$(dpkg-query -W -f='${Version}' libwebkit2gtk-4.1-0 2>/dev/null)
    ;;
  fedora-42|fedora-43)
    pkgs="webkit2gtk4.1 xorg-x11-server-Xvfb xorg-x11-xauth"
    run "$pkgs" dnf -y --setopt=install_weak_deps=False install $pkgs
    webkit=$(rpm -q --qf '%{VERSION}' webkit2gtk4.1 2>/dev/null)
    ;;
  almalinux-9)
    pkgs="webkit2gtk4.1 xorg-x11-server-Xvfb xorg-x11-xauth"
    run "$pkgs" dnf -y --setopt=install_weak_deps=False install $pkgs
    webkit=$(rpm -q --qf '%{VERSION}' webkit2gtk4.1 2>/dev/null)
    ;;
  arch)
    pkgs="webkit2gtk-4.1 xorg-server-xvfb xorg-xauth"
    run "$pkgs" sh -c "pacman -Sy --noconfirm --needed $pkgs"
    webkit=$(pacman -Q webkit2gtk-4.1 2>/dev/null | cut -d' ' -f2)
    ;;
  opensuse-tumbleweed)
    pkgs="libwebkit2gtk-4_1-0 xvfb-run xauth"
    run "$pkgs" sh -c "zypper -n --gpg-auto-import-keys refresh && zypper -n install --no-recommends $pkgs"
    webkit=$(rpm -q --qf '%{VERSION}' libwebkit2gtk-4_1-0 2>/dev/null)
    ;;
  alpine)
    pkgs="webkit2gtk-4.1 xvfb-run xvfb xauth"
    run "$pkgs" apk add --no-cache $pkgs
    webkit=$(apk info -v webkit2gtk-4.1 2>/dev/null | head -n1)
    ;;
  *)
    fail "distro desconocida: $distro"
    ;;
esac

rm -f "$log"
command -v xvfb-run >/dev/null || fail "xvfb-run no está tras instalar ($pkgs)"
echo "WEBKIT=${webkit:-desconocida}"
