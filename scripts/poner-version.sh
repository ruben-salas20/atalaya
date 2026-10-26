#!/usr/bin/env bash
# Escribe la versión X.Y.Z en Cargo.toml y Cargo.lock (#62). La usa el flujo
# «Publicar versión» antes de compilar una etiqueta: el número de la versión
# sale de la etiqueta, no de main. Android lo lee de Cargo.toml.
#   scripts/poner-version.sh 0.2.2
set -euo pipefail

v=${1:?Uso: scripts/poner-version.sh X.Y.Z}
[[ "$v" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "Versión no válida: $v" >&2; exit 1; }
root=$(cd "$(dirname "$0")/.." && pwd)

sed -i "/^\[workspace.package\]/,/^\[/s/^version = \".*\"/version = \"$v\"/" "$root/Cargo.toml"
# En Cargo.lock, la línea siguiente a cada `name = "atalaya-…"` es su versión.
sed -i -E "/^name = \"atalaya-(api|desktop|server)\"$/{n;s/^version = \".*\"/version = \"$v\"/}" "$root/Cargo.lock"
