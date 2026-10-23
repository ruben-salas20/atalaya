#!/usr/bin/env bash
# Compila la app en ai-lab dentro de un contenedor temporal y trae el APK.
#
#   android/compilar.sh                  → assembleDebug
#   android/compilar.sh test             → pruebas unitarias
#   android/compilar.sh <tareas Gradle>  → las tareas que se pidan
#
# Deja el APK en android/app/build/outputs/apk/debug/app-debug.apk.
set -euo pipefail

HOST=ai-lab
IMAGE=atalaya-android-build:local
CACHE_VOLUME=atalaya-android-gradle

here="$(cd "$(dirname "$0")" && pwd)"
branch="$(git -C "$here" branch --show-current | tr '/' '-')"
remote_dir="atalaya-build/${branch:-sin-rama}"

case "${1:-}" in
  "") tasks=(assembleDebug) ;;
  test) tasks=(testDebugUnitTest) ;;
  *) tasks=("$@") ;;
esac

# La imagen se crea la primera vez o cuando cambia su receta.
recipe_hash="$(sha256sum "$here/build.Dockerfile" | cut -c1-12)"
if [[ "$(ssh "$HOST" "docker image inspect -f '{{index .Config.Labels \"recipe\"}}' $IMAGE 2>/dev/null" || true)" != "$recipe_hash" ]]; then
  echo "→ creando la imagen $IMAGE en $HOST"
  ssh "$HOST" "docker build -q --label recipe=$recipe_hash -t $IMAGE -" < "$here/build.Dockerfile"
fi

echo "→ copiando android/ a $HOST:~/$remote_dir"
ssh "$HOST" "mkdir -p ~/$remote_dir"
rsync -az --delete --exclude build/ --exclude .gradle/ --exclude local.properties \
  "$here/" "$HOST:$remote_dir/"

echo "→ gradle ${tasks[*]}"
ssh "$HOST" "docker run --rm --memory 6g --cpus 4 \
  --user \$(id -u):\$(id -g) \
  -v \$HOME/$remote_dir:/src -v $CACHE_VOLUME:/gradle \
  $IMAGE ./gradlew --no-daemon --console=plain ${tasks[*]}"

if [[ " ${tasks[*]} " == *" assembleDebug "* ]]; then
  mkdir -p "$here/app/build/outputs/apk/debug"
  scp -q "$HOST:$remote_dir/app/build/outputs/apk/debug/app-debug.apk" "$here/app/build/outputs/apk/debug/"
  echo "→ APK en android/app/build/outputs/apk/debug/app-debug.apk"
fi
