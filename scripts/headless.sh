#!/usr/bin/env bash
# Prueba una app gráfica en un sway sin pantalla, con ratón y teclado virtuales.
# No toca la pantalla real. Cada sesión tiene nombre (variable HEADLESS, por defecto
# «pruebas»), así que dos agentes pueden probar a la vez con nombres distintos.
#
#   scripts/headless.sh start <comando> [args...]   arranca sway y la app (1440x900)
#   scripts/headless.sh mouse "move X Y" "click" ...  órdenes del ratón: move X Y,
#                                                   click [left|right|middle], scroll N (pasos), sleep MS
#   scripts/headless.sh type "texto"                escribe en lo que tenga el foco
#   scripts/headless.sh key Return                  pulsa una tecla (nombres de xkb)
#   scripts/headless.sh shot salida.png             captura la pantalla entera
#   scripts/headless.sh log                         muestra el log de la app
#   scripts/headless.sh stop                        cierra todo y borra la sesión
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
name=${HEADLESS:-pruebas}
dir=/tmp/atalaya-headless/$name
export XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-/run/user/$(id -u)}
mouse_bin=$root/scripts/raton-virtual/target/release/raton-virtual

die() { echo "$*" >&2; exit 1; }
need_session() {
  [ -s "$dir/wd" ] || die "no hay sesión «$name»; usa: scripts/headless.sh start <comando>"
  export WAYLAND_DISPLAY=$(cat "$dir/wd")
  unset DISPLAY
}

cmd=${1:-}; shift || true
case "$cmd" in
  start)
    [ $# -gt 0 ] || die "falta el comando de la app"
    [ -e "$dir" ] && die "la sesión «$name» ya existe; ciérrala con stop"
    if [ ! -x "$mouse_bin" ]; then
      cargo build --release --quiet --manifest-path "$root/scripts/raton-virtual/Cargo.toml"
    fi
    mkdir -p "$dir"
    printf 'output HEADLESS-1 resolution 1440x900\nexec sh -c "echo \\$WAYLAND_DISPLAY > %s/wd"\n' "$dir" > "$dir/config"
    env -u WAYLAND_DISPLAY -u DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
      WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 \
      setsid sway -c "$dir/config" > "$dir/sway.log" 2>&1 < /dev/null &
    echo $! > "$dir/sway.pid"
    for _ in $(seq 40); do [ -s "$dir/wd" ] && break; sleep 0.25; done
    [ -s "$dir/wd" ] || { cat "$dir/sway.log" >&2; kill "$(cat "$dir/sway.pid")"; rm -rf "$dir"; die "sway no arrancó"; }
    need_session
    # El ratón vive mientras alguien tenga abierta la tubería; «sleep» la mantiene abierta.
    mkfifo "$dir/mouse"
    setsid "$mouse_bin" 1440x900 < "$dir/mouse" > "$dir/mouse.log" 2>&1 &
    echo $! > "$dir/mouse.pid"
    setsid sleep infinity > "$dir/mouse" &
    echo $! > "$dir/hold.pid"
    setsid "$@" > "$dir/app.log" 2>&1 < /dev/null &
    echo $! > "$dir/app.pid"
    echo "sesión «$name» en marcha (WAYLAND_DISPLAY=$WAYLAND_DISPLAY)"
    ;;
  mouse)
    need_session
    for order in "$@"; do echo "$order"; done > "$dir/mouse"
    sleep 0.3
    ;;
  type)
    need_session
    # La pausa inicial evita que se pierda la primera letra mientras la app registra el teclado.
    wtype -s 150 "$1"
    ;;
  key)
    need_session
    wtype -s 150 -k "$1"
    ;;
  shot)
    need_session
    grim -o HEADLESS-1 "${1:?falta la ruta de la captura}"
    echo "captura: $1"
    ;;
  log)
    cat "$dir/app.log"
    ;;
  stop)
    [ -d "$dir" ] || die "no hay sesión «$name»"
    for p in app hold mouse sway; do
      [ -s "$dir/$p.pid" ] && kill "$(cat "$dir/$p.pid")" 2>/dev/null || true
    done
    echo "--- últimas líneas del log de la app ---"
    tail -n 20 "$dir/app.log" 2>/dev/null || true
    rm -rf "$dir"
    ;;
  *)
    sed -n '2,15p' "$0"
    exit 1
    ;;
esac
