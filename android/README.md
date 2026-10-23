# App de Android

Kotlin y Jetpack Compose (ADR 0006). Paquete `dev.rubensalas.atalaya`, Android 14 o posterior.

## Compilar

El portátil no tiene Java ni el SDK de Android. Se compila en `ai-lab`, dentro de un contenedor temporal:

```bash
android/compilar.sh                  # APK de depuración
android/compilar.sh test             # pruebas unitarias
android/compilar.sh assembleDebug testDebugUnitTest
```

El script copia `android/` a `ai-lab:~/atalaya-build/<rama>/`, corre Gradle en la imagen `atalaya-android-build:local` (receta en `build.Dockerfile`) y trae el APK a `android/app/build/outputs/apk/debug/app-debug.apk`. La primera vez tarda unos 5 minutos; después, 1 o 2.

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
| `data/papers/` | Búsqueda y guardado de papers, siempre contra el servicio, y las búsquedas recientes, guardadas solo en el móvil |
| `ui/theme/` | Colores de las maquetas, Roboto y el tamaño de letra. Los grises se piden con `Atalaya.colors` |
| `ui/navigation/` | Las rutas entre pantallas |
| `ui/settings/`, `ui/list/`, `ui/article/`, `ui/feeds/`, `ui/papers/` | Una carpeta por grupo de pantallas |

Las dependencias van todas en `gradle/libs.versions.toml`, con versión fija.
