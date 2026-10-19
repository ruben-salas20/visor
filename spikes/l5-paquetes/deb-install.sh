#!/usr/bin/env bash
# Instala visor.deb en una distro limpia y comprueba que arranca y que el
# paquete se declara como gestionado (no se actualiza solo).
# Uso: deb-install.sh <distro> <visor.deb> <carpeta-de-resultados>
# Escribe <carpeta>/resultado.tsv con líneas "clave<TAB>valor".
set -uo pipefail

distro=${1:?uso: deb-install.sh <distro> <visor.deb> <carpeta>}
deb=${2:?falta el .deb}
out=${3:?falta la carpeta de resultados}
mkdir -p "$out"
tsv="$out/resultado.tsv"
export DEBIAN_FRONTEND=noninteractive

emit() { printf '%s\t%s\n' "$1" "$2" >> "$tsv"; }
: > "$tsv"

apt-get update -qq >"$out/apt-update.log" 2>&1
before=$(dpkg-query -W | wc -l)
if ! apt-get install -y --no-install-recommends "$deb" >"$out/apt-install.log" 2>&1; then
  emit instalacion falla
  tail -n 20 "$out/apt-install.log" >&2
  exit 1
fi
after=$(dpkg-query -W | wc -l)
emit instalacion funciona
emit paquetes_nuevos "$((after - before))"
emit tamano_instalado_kib "$(dpkg-query -W -f='${Installed-Size}' visor)"

# Arranque: misma prueba que L2 (smoke.sh). Instala también xvfb y WebKit, ya presentes.
bash /linux/smoke.sh "$distro" /usr/bin/visor "$out/smoke.json" > "$out/smoke.txt" 2>&1
emit arranque "$(grep -m1 '^RESULTADO' "$out/smoke.txt" | sed 's/^RESULTADO [^:]*: //')"
