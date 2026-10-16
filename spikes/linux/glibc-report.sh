#!/usr/bin/env bash
# Versión máxima de glibc que exige un binario y los símbolos que la piden.
# Uso: glibc-report.sh <binario>          informe en Markdown
#      glibc-report.sh <binario> --max    solo la versión, p. ej. 2.39
set -euo pipefail

bin=${1:?uso: glibc-report.sh <binario> [--max]}
mode=${2:-}

# objdump -T marca cada símbolo importado con su versión: "(GLIBC_2.39) pidfd_spawnp".
dyn=$(objdump -T "$bin")
versions=$(printf '%s\n' "$dyn" | grep -o '(GLIBC_[0-9.]*)' | tr -d '()' | sort -uV || true)
max=$(printf '%s\n' "$versions" | sed '/^$/d' | tail -n1)
max=${max#GLIBC_}

if [ "$mode" = "--max" ]; then
  echo "${max:-ninguna}"
  exit 0
fi

echo "## glibc exigida por \`$(basename "$bin")\`"
echo
host=$(ldd --version 2>/dev/null || true)
echo "- Sistema donde se compiló: ${host%%$'\n'*}"
echo "- Versión máxima exigida: **${max:-ninguna}**"
echo
if [ -n "$max" ]; then
  echo "### Símbolos que exigen $max"
  echo
  echo '```'
  printf '%s\n' "$dyn" | grep -F "(GLIBC_$max)" | awk '{print $NF}' | sort -u
  echo '```'
  echo
fi
echo "### Versiones que pide cada biblioteca (\`readelf -V\`)"
echo
echo '```'
readelf -V "$bin" | sed -n '/Version needs/,/^$/p'
echo '```'
