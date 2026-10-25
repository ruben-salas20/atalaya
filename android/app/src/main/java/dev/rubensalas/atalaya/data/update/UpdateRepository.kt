package dev.rubensalas.atalaya.data.update

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.Settings
import dev.rubensalas.atalaya.data.api.ApiException
import dev.rubensalas.atalaya.data.settings.SettingsRepository
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import java.io.File
import java.io.IOException
import java.time.Instant

/** Lo que ven la lista y Ajustes sobre la actualización. */
data class UpdateState(
    /** Versión nueva que se ofrece. Null si no hay ninguna. */
    val available: Release? = null,
    /** El aviso se cerró con la ✕. Vuelve al abrir la app con otra comprobación. */
    val bannerClosed: Boolean = false,
    /** Descargando o preparando la instalación. */
    val busy: Boolean = false,
    /** Android aún no deja instalar desde Atalaya; se espera a que el usuario lo permita. */
    val waitingPermission: Boolean = false,
    /** Lo que salió mal la última vez, en español. */
    val error: String? = null,
)

/**
 * Busca versiones nuevas y las instala (ADR 0012). Vive en el contenedor de la
 * app, así la descarga sigue aunque se cambie de pantalla. [scope] no se cancela.
 */
class UpdateRepository(
    private val context: Context,
    private val settings: SettingsRepository,
    private val releases: ReleaseClient,
    private val checker: UpdateChecker,
    private val scope: CoroutineScope,
) {
    private val _state = MutableStateFlow(UpdateState())
    val state: StateFlow<UpdateState> = _state.asStateFlow()

    /**
     * Al abrir la app: comprueba si toca (como mucho cada seis horas). Sin red o
     * con error no dice nada; se vuelve a intentar en la próxima apertura.
     */
    fun checkOnOpen() {
        scope.launch {
            if (isAutoCheckDue(settings.current().lastUpdateCheckAt, Instant.now())) checkNow()
        }
    }

    /**
     * Pide la última Release y actualiza el aviso. Lo usan «Buscar actualizaciones»
     * y la comprobación al abrir. Solo guarda la hora si no hay nada nuevo: mientras
     * haya una versión pendiente, cada apertura vuelve a mirar y el aviso no se pierde
     * al cerrar la app.
     */
    suspend fun checkNow(): UpdateCheck {
        val result = checker.check()
        when (result) {
            is UpdateCheck.Available -> _state.update {
                // Si es la misma versión que ya se cerró, sigue cerrada.
                it.copy(available = result.release, bannerClosed = it.available?.tag == result.release.tag && it.bannerClosed)
            }
            UpdateCheck.UpToDate -> _state.update { it.copy(available = null, bannerClosed = false) }
            is UpdateCheck.Failed -> Unit
        }
        if (result == UpdateCheck.UpToDate) settings.markUpdateChecked(Instant.now())
        return result
    }

    fun closeBanner() = _state.update { it.copy(bannerClosed = true) }

    /**
     * Descarga, comprueba e instala la versión ofrecida. Devuelve false si Android
     * aún no deja instalar desde Atalaya: la pantalla debe abrir
     * [unknownSourcesIntent]; al volver, [onResumed] sigue sola.
     */
    fun install(): Boolean {
        val current = _state.value
        val release = current.available ?: return true
        if (current.busy) return true
        if (!context.packageManager.canRequestPackageInstalls()) {
            _state.update { it.copy(waitingPermission = true) }
            return false
        }
        _state.update { it.copy(busy = true, waitingPermission = false, error = null) }
        scope.launch(Dispatchers.IO) {
            try {
                val apk = downloadVerified(release)
                ApkInstaller.commit(context, apk)
                // PackageInstaller ya copió el APK a su sesión: el de la caché sobra.
                apk.parentFile?.deleteRecursively()
            } catch (e: ApiException) {
                _state.update { it.copy(busy = false, error = e.message) }
            } catch (e: IOException) {
                _state.update { it.copy(busy = false, error = "No se pudo leer la descarga. Pulsa Actualizar otra vez") }
            }
        }
        return true
    }

    /** Pantalla de Android donde se permite instalar desde Atalaya. */
    fun unknownSourcesIntent(): Intent =
        Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${context.packageName}"))

    /** Al volver a la app: si estaba esperando ese permiso y ya lo tiene, sigue la actualización. */
    fun onResumed() {
        if (_state.value.waitingPermission) install()
    }

    /** Llega del receptor de PackageInstaller. [error] es null si la instalación pidió confirmación o salió bien. */
    fun onInstallResult(error: String?) = _state.update { it.copy(busy = false, error = error) }

    /** Baja el APK y su SHA256SUMS a la caché y comprueba la huella. Si no cuadra, borra todo. */
    private suspend fun downloadVerified(release: Release): File {
        val dir = File(context.cacheDir, "actualizacion").apply {
            deleteRecursively()
            mkdirs()
        }
        val apk = File(dir, APK_NAME)
        val sums = File(dir, SUMS_NAME)
        try {
            releases.download(release.apkUrl, apk)
            releases.download(release.sumsUrl, sums)
        } catch (e: ApiException) {
            dir.deleteRecursively()
            throw e
        }
        val expected = expectedSha256(sums.readText(), APK_NAME)
        if (expected == null) {
            dir.deleteRecursively()
            throw ApiException(null, "La versión ${release.tag} no trae la huella de $APK_NAME")
        }
        if (sha256Hex(apk) != expected) {
            dir.deleteRecursively()
            throw ApiException(null, "La descarga no coincide con su huella. Se ha borrado: pulsa Actualizar otra vez")
        }
        return apk
    }
}
