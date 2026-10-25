package dev.rubensalas.atalaya.data.update

import dev.rubensalas.atalaya.data.api.ApiException
import java.time.Duration
import java.time.Instant

/** Resultado de buscar una versión nueva. */
sealed interface UpdateCheck {
    /** La Release publicada no es mayor que la instalada. */
    data object UpToDate : UpdateCheck

    data class Available(val release: Release) : UpdateCheck

    /** No se pudo comprobar. [message] está en español y se puede mostrar tal cual. */
    data class Failed(val message: String) : UpdateCheck
}

/** Compara la última Release con la versión instalada. */
class UpdateChecker(
    private val releases: ReleaseClient,
    /** La versión instalada: `BuildConfig.VERSION_NAME`. */
    private val installed: Version,
) {
    suspend fun check(): UpdateCheck = try {
        val release = releases.latest()
        if (release.version > installed) UpdateCheck.Available(release) else UpdateCheck.UpToDate
    } catch (e: ApiException) {
        UpdateCheck.Failed(e.message.orEmpty())
    }
}

/** Como mucho una comprobación automática cada seis horas (ADR 0012). */
val AUTO_CHECK_EVERY: Duration = Duration.ofHours(6)

/**
 * ¿Toca comprobar al abrir la app? Sí si nunca se ha hecho, si pasaron seis
 * horas desde la última o si la hora guardada está en el futuro (el reloj cambió).
 */
fun isAutoCheckDue(lastCheckedAt: Instant?, now: Instant): Boolean {
    if (lastCheckedAt == null) return true
    val elapsed = Duration.between(lastCheckedAt, now)
    return elapsed.isNegative || elapsed >= AUTO_CHECK_EVERY
}
