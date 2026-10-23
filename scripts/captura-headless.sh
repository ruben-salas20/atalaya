#!/usr/bin/env bash
# Uso: scripts/captura-headless.sh <salida.png> <segundos> <comando> [args...]
# Abre <comando> en un sway sin pantalla (1440x900), espera y captura. No toca la pantalla real.
set -euo pipefail
out=$1; wait_s=$2; shift 2
export XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-/run/user/$(id -u)}
dir=$(mktemp -d)
printf 'output HEADLESS-1 resolution 1440x900\nexec sh -c "echo \\$WAYLAND_DISPLAY > %s/wd"\n' "$dir" > "$dir/config"
env -u WAYLAND_DISPLAY -u DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 \
  sway -c "$dir/config" > "$dir/sway.log" 2>&1 &
sway_pid=$!
trap 'kill $sway_pid 2>/dev/null || true' EXIT
for _ in $(seq 40); do [ -s "$dir/wd" ] && break; sleep 0.25; done
[ -s "$dir/wd" ] || { echo "sway no arrancó; log: $dir/sway.log" >&2; exit 1; }
export WAYLAND_DISPLAY=$(cat "$dir/wd")
unset DISPLAY
"$@" > "$dir/app.log" 2>&1 &
app_pid=$!
sleep "$wait_s"
grim -o HEADLESS-1 "$out"
kill $app_pid 2>/dev/null || true
echo "captura: $out · log de la app: $dir/app.log"
