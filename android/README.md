# App de Android

Kotlin y Jetpack Compose (ADR 0006). Paquete `dev.rubensalas.atalaya`, Android 14 o posterior.

## Compilar

Se compila en `ai-lab`, con el JDK 21 y el SDK de Android de `/data/bots` (`/data/bots/android-env.sh`):

```bash
android/compilar.sh                  # APK de depuración
android/compilar.sh test             # pruebas unitarias
android/compilar.sh assembleDebug testDebugUnitTest
```

El APK queda en `android/app/build/outputs/apk/debug/app-debug.apk`. La versión que se publica la compila GitHub Actions, firmada (ADR 0012).

## Probar en el emulador

`/data/bots/emulador.sh` arranca sin ventana un Android 16 (`atalaya-a35`). Después, `adb install -r` del APK y `adb exec-out screencap -p > captura.png` para ver la pantalla. Se apaga con `adb emu kill`. Usa unos 2 GB de memoria: no lo arranques mientras compila Gradle.

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
