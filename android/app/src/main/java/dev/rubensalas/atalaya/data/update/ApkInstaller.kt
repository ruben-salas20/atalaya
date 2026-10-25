package dev.rubensalas.atalaya.data.update

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import dev.rubensalas.atalaya.data.api.ApiException
import java.io.File
import java.io.IOException

/** Entrega un APK al instalador de Android (`PackageInstaller`, ADR 0012). */
object ApkInstaller {
    /**
     * Crea una sesión de instalación con [apk] y la confirma. El resultado llega
     * a [InstallResultReceiver]. Lanza [ApiException] si no se pudo empezar.
     */
    fun commit(context: Context, apk: File) {
        val installer = context.packageManager.packageInstaller
        val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL).apply {
            setAppPackageName(context.packageName)
            // Pide que no haga falta confirmar. Android solo lo respeta si la app
            // ya se actualiza a sí misma; si no, el resultado trae una pantalla que abrir.
            setRequireUserAction(PackageInstaller.SessionParams.USER_ACTION_NOT_REQUIRED)
        }
        var sessionId: Int? = null
        try {
            val id = installer.createSession(params)
            sessionId = id
            installer.openSession(id).use { session ->
                session.openWrite(APK_NAME, 0, apk.length()).use { out ->
                    apk.inputStream().use { input -> input.copyTo(out) }
                    session.fsync(out)
                }
                // Mutable: Android tiene que poder añadir el estado y la pantalla de confirmación.
                val intent = Intent(context, InstallResultReceiver::class.java)
                val pending = PendingIntent.getBroadcast(
                    context,
                    id,
                    intent,
                    PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE,
                )
                session.commit(pending.intentSender)
            }
        } catch (e: IOException) {
            abandon(installer, sessionId)
            throw ApiException(null, "No se pudo preparar la instalación", e)
        } catch (e: SecurityException) {
            abandon(installer, sessionId)
            throw ApiException(null, "Android no dejó preparar la instalación", e)
        }
    }

    /** Deshace una sesión a medias para que no quede ocupando sitio. */
    private fun abandon(installer: PackageInstaller, sessionId: Int?) {
        if (sessionId != null) runCatching { installer.abandonSession(sessionId) }
    }
}
