#!/usr/bin/env bash
# Instala la app de PC para el usuario, sin sudo:
# - el programa en ~/.local/bin/atalaya-desktop;
# - el icono en ~/.local/share/icons/hicolor/scalable/apps/atalaya.svg;
# - la entrada del lanzador de Omarchy en ~/.local/share/applications/atalaya.desktop.
#
#   scripts/instalar-pc.sh            → compila el repo con cargo
#   scripts/instalar-pc.sh --release  → descarga la última versión publicada
#
# Con --release no hace falta el repo (ADR 0012):
#   curl -fsSL https://raw.githubusercontent.com/ruben-salas20/atalaya/main/scripts/instalar-pc.sh | bash -s -- --release
set -euo pipefail

REPO=ruben-salas20/atalaya
BIN=atalaya-desktop-linux-x86_64

share=${XDG_DATA_HOME:-$HOME/.local/share}
# Vacío cuando el script llega por curl: entonces no hay repo.
root=""
if [[ -n "${BASH_SOURCE[0]:-}" && -f "${BASH_SOURCE[0]}" ]]; then
  root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
fi

case "${1:-}" in
  --release)
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    echo "→ descargando la última versión de $REPO"
    curl -fsSL -o "$tmp/$BIN" "https://github.com/$REPO/releases/latest/download/$BIN"
    curl -fsSL -o "$tmp/SHA256SUMS" "https://github.com/$REPO/releases/latest/download/SHA256SUMS"
    (cd "$tmp" && grep " $BIN\$" SHA256SUMS | sha256sum -c --quiet -)
    install -Dm755 "$tmp/$BIN" "$HOME/.local/bin/atalaya-desktop"
    ;;
  "")
    [[ -n "$root" ]] || { echo "Sin el repo, usa --release." >&2; exit 1; }
    cargo install --locked --path "$root/crates/atalaya-desktop" --root "$HOME/.local"
    ;;
  *)
    echo "Uso: $0 [--release]" >&2
    exit 1
    ;;
esac

icon="$share/icons/hicolor/scalable/apps/atalaya.svg"
if [[ -n "$root" && -f "$root/assets/icono/atalaya.svg" ]]; then
  install -Dm644 "$root/assets/icono/atalaya.svg" "$icon"
else
  mkdir -p "$(dirname "$icon")"
  curl -fsSL -o "$icon" "https://raw.githubusercontent.com/$REPO/main/assets/icono/atalaya.svg"
fi

# StartupWMClass es el app_id de la ventana (main.rs): así Hyprland y el
# lanzador saben que la ventana es de esta entrada.
install -Dm644 /dev/stdin "$share/applications/atalaya.desktop" <<EOF
[Desktop Entry]
Name=Atalaya
Comment=Lector RSS que traduce al español
Exec=$HOME/.local/bin/atalaya-desktop
Icon=atalaya
StartupWMClass=atalaya
Categories=Network;News;
Terminal=false
Type=Application
EOF

update-desktop-database "$share/applications" 2>/dev/null || true
echo "Instalada. Ábrela desde el lanzador: Atalaya."
