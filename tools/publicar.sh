#!/usr/bin/env bash
# Publica la versión de Cargo.toml como release de GitHub, con el binario que usa la actualización automática.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
[ -z "$(git status --porcelain)" ] || { echo "Hay cambios sin confirmar: confírmalos antes de publicar." >&2; exit 1; }
git rev-parse -q --verify "refs/tags/v$VERSION" >/dev/null && { echo "La etiqueta v$VERSION ya existe: sube la versión en Cargo.toml." >&2; exit 1; }

cargo build --release
DIR=$(mktemp -d)
trap 'rm -rf "$DIR"' EXIT
cp target/release/visor "$DIR/visor-x86_64-linux"

NOTES=${1:-"visor $VERSION"}
git tag "v$VERSION" && git push origin "v$VERSION" && gh release create "v$VERSION" "$DIR/visor-x86_64-linux" --title "visor $VERSION" --notes "$NOTES"
