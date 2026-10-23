#!/usr/bin/env bash
# Instala la app de PC para el usuario, sin sudo:
# - el programa en ~/.local/bin/atalaya-desktop;
# - el icono en ~/.local/share/icons/hicolor/scalable/apps/atalaya.svg;
# - la entrada del lanzador de Omarchy en ~/.local/share/applications/atalaya.desktop.
# Se vuelve a correr para actualizar.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
share=${XDG_DATA_HOME:-$HOME/.local/share}

cargo install --locked --path "$root/crates/atalaya-desktop" --root "$HOME/.local"

install -Dm644 "$root/assets/icono/atalaya.svg" "$share/icons/hicolor/scalable/apps/atalaya.svg"

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
