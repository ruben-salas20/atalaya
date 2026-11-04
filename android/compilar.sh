#!/usr/bin/env bash
# Compila la app dentro de Docker, sin instalar Java ni el SDK de Android.
#
#   android/compilar.sh                  → assembleDebug
#   android/compilar.sh test             → pruebas unitarias
#   android/compilar.sh <tareas Gradle>  → las tareas que se pidan
#
# Deja el APK en android/app/build/outputs/apk/debug/app-debug.apk.
# Para firmar en local: ATALAYA_KEYSTORE=<ruta del .jks> y
# ATALAYA_KEYSTORE_PASSWORD=<contraseña> en el entorno (ver README.md).
# La caché de Gradle vive en el volumen atalaya-android-gradle.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
imagen=atalaya-android-build:local

# La imagen se crea la primera vez y cada vez que cambia su receta.
receta="$(sha256sum "$here/build.Dockerfile" | cut -c1-12)"
if [[ "$(docker image inspect -f '{{index .Config.Labels "receta"}}' "$imagen" 2>/dev/null || true)" != "$receta" ]]; then
  echo "→ creando la imagen $imagen"
  docker build -q --label receta="$receta" -t "$imagen" -f "$here/build.Dockerfile" "$here" > /dev/null
fi

case "${1:-}" in
  "") tasks=(assembleDebug) ;;
  test) tasks=(testDebugUnitTest) ;;
  *) tasks=("$@") ;;
esac

firma=()
if [[ -n "${ATALAYA_KEYSTORE:-}" ]]; then
  firma=(-v "$(realpath "$ATALAYA_KEYSTORE"):/firma/atalaya.jks:ro"
         -e ATALAYA_KEYSTORE=/firma/atalaya.jks -e ATALAYA_KEYSTORE_PASSWORD)
fi

# Se monta el repo entero, no solo android/: Gradle lee la versión de ../Cargo.toml.
# 6 GB y 4 CPU, como Tinta: ai-lab comparte la memoria con llama-swap.
# Gradle con 3 GB y el compilador de Kotlin dentro del mismo proceso caben.
exec docker run --rm -i --memory=6g --cpus=4 \
  --user "$(id -u):$(id -g)" -e HOME=/tmp -e GRADLE_USER_HOME=/gradle \
  -v atalaya-android-gradle:/gradle -v "$(dirname "$here")":/src -w /src/android \
  ${firma[@]+"${firma[@]}"} \
  "$imagen" ./gradlew --no-daemon --console=plain --max-workers=2 \
  -Dorg.gradle.jvmargs="-Xmx3g -XX:MaxMetaspaceSize=1g -Dfile.encoding=UTF-8" \
  -Pkotlin.compiler.execution.strategy=in-process "${tasks[@]}"
