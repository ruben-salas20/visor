#!/usr/bin/env bash
# Publishes the version in Cargo.toml: creates and pushes the tag v<version>, whose
# message becomes the release notes. GitHub Actions (.github/workflows/release.yml)
# then builds the Linux and Windows files and creates the release. A version with
# a «-» (0.3.0-rc) becomes a pre-release.
# Usage: tools/publicar.sh ["release notes"]
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
TAG="v$VERSION"
[ -z "$(git status --porcelain)" ] || { echo "Hay cambios sin confirmar: confírmalos antes de publicar." >&2; exit 1; }
git rev-parse -q --verify "refs/tags/$TAG" >/dev/null && { echo "La etiqueta $TAG ya existe: sube la versión en Cargo.toml." >&2; exit 1; }
git fetch -q origin
[ -z "$(git ls-remote --tags origin "refs/tags/$TAG")" ] || { echo "La etiqueta $TAG ya existe en GitHub: sube la versión en Cargo.toml." >&2; exit 1; }
# The workflow builds exactly this commit; it must be one that is already on GitHub.
[ -n "$(git branch -r --contains HEAD)" ] || { echo "Este commit no está en ninguna rama de GitHub: haz push antes de publicar." >&2; exit 1; }

NOTES=${1:-"visor $VERSION"}
git tag -a "$TAG" -m "$NOTES"
git push origin "$TAG"
echo "Etiqueta $TAG subida. GitHub Actions compila y crea la release:"
echo "  gh run watch \$(gh run list --workflow release.yml --limit 1 --json databaseId --jq '.[0].databaseId')"
