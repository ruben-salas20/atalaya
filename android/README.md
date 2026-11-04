# App de Android

Kotlin y Jetpack Compose (ADR 0006). Paquete `dev.rubensalas.atalaya`, Android 14 o posterior.

## Compilar

Se compila dentro de Docker, sin instalar Java ni el SDK de Android. Solo hace falta Docker:

```bash
android/compilar.sh                  # APK de depuración
android/compilar.sh test             # pruebas unitarias
android/compilar.sh assembleDebug testDebugUnitTest
```

El APK queda en `android/app/build/outputs/apk/debug/app-debug.apk`.

- **La primera vez** el script crea la imagen `atalaya-android-build:local` con `build.Dockerfile`: JDK 21 y SDK 37, las mismas versiones que GitHub Actions. La vuelve a crear cuando cambia la receta.
- **La caché de Gradle** vive en el volumen `atalaya-android-gradle`. Para liberar espacio se pueden borrar la imagen y el volumen: el script los recrea.
- **El contenedor** usa como mucho 6 GB de memoria y 4 CPU.

### Firmar en local

La versión que se publica la compila y firma GitHub Actions (ADR 0012). Para compilar una firmada en local, pasa la clave por el entorno. La clave nunca entra en el repo: en ai-lab está en `~/.config/atalaya-android/`.

```bash
export ATALAYA_KEYSTORE=~/.config/atalaya-android/atalaya.jks
export ATALAYA_KEYSTORE_PASSWORD="$(cat ~/.config/atalaya-android/clave)"
android/compilar.sh assembleRelease
```

El APK queda en `android/app/build/outputs/apk/release/app-release.apk`. Sin esas variables, `assembleRelease` sale sin firmar.

## Probar

No hay emulador en ai-lab. La app se prueba en el móvil.

## Instalar en el móvil

```bash
adb install -r android/app/build/outputs/apk/debug/app-debug.apk
```

## Dónde está cada cosa

| Carpeta en `app/src/main/java/dev/rubensalas/atalaya/` | Qué tiene |
|---|---|
| `data/api/` | El contrato de la API en Kotlin (`Models.kt`, copia de `crates/atalaya-api`) y el cliente HTTP (`AtalayaClient.kt`) |
| `data/settings/` | Los ajustes, guardados con DataStore |
| `data/list/` | De dónde salen la lista y los artículos. `OfflineListRepository` junta el servicio con la copia del móvil |
| `data/local/` | La base Room del móvil: filas de la lista, texto de guardados y abiertos, y el menú lateral |
| `data/sync/` | La sincronización en segundo plano con WorkManager: cada 30 minutos y al abrir la app |
| `data/update/` | Buscar la última versión en GitHub Releases, bajar el APK, comprobar su huella e instalarlo con `PackageInstaller` (ADR 0012) |
| `data/papers/` | Búsqueda y guardado de papers, siempre contra el servicio, y las búsquedas recientes, guardadas solo en el móvil |
| `ui/theme/` | Colores de las maquetas, Roboto y el tamaño de letra. Los grises se piden con `Atalaya.colors` |
| `ui/update/` | El aviso de versión nueva de la lista |
| `ui/navigation/` | Las rutas entre pantallas |
| `ui/settings/`, `ui/list/`, `ui/article/`, `ui/feeds/`, `ui/papers/` | Una carpeta por grupo de pantallas |

Las dependencias van todas en `gradle/libs.versions.toml`, con versión fija.
