#!/usr/bin/env bash
# Compila la app en ai-lab con el SDK de /data/bots, sin Docker.
#
#   android/compilar.sh                  → assembleDebug
#   android/compilar.sh test             → pruebas unitarias
#   android/compilar.sh <tareas Gradle>  → las tareas que se pidan
#
# Deja el APK en android/app/build/outputs/apk/debug/app-debug.apk.
set -euo pipefail

. /data/bots/android-env.sh
here="$(cd "$(dirname "$0")" && pwd)"

case "${1:-}" in
  "") tasks=(assembleDebug) ;;
  test) tasks=(testDebugUnitTest) ;;
  *) tasks=("$@") ;;
esac

# La sesión del agente tiene un límite de 6 GB de memoria: Gradle con 2 GB y
# el compilador de Kotlin dentro del mismo proceso caben; con más, la matan.
cd "$here"
exec ./gradlew --no-daemon --max-workers=2 \
  -Dorg.gradle.jvmargs="-Xmx2g -XX:MaxMetaspaceSize=768m -Dfile.encoding=UTF-8" \
  -Pkotlin.compiler.execution.strategy=in-process "${tasks[@]}"
