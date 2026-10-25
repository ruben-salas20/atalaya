package dev.rubensalas.atalaya.data.update

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import dev.rubensalas.atalaya.AtalayaApp

/**
 * Recibe el resultado de `PackageInstaller` (ADR 0012). Solo lo manda el sistema:
 * el receptor no se exporta en el manifiesto.
 */
class InstallResultReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val updates = (context.applicationContext as AtalayaApp).container.updates
        val status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)
        when (status) {
            PackageInstaller.STATUS_PENDING_USER_ACTION -> {
                // Android pide un toque para confirmar: se abre su pantalla.
                val confirm = intent.getParcelableExtra(Intent.EXTRA_INTENT, Intent::class.java)
                if (confirm == null) {
                    updates.onInstallResult("Android no enseñó la confirmación de la instalación")
                } else {
                    context.startActivity(confirm.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                }
            }
            // Con éxito, Android reinicia la app con la versión nueva.
            PackageInstaller.STATUS_SUCCESS -> updates.onInstallResult(null)
            else -> updates.onInstallResult(installErrorText(status))
        }
    }

    private fun installErrorText(status: Int): String = when (status) {
        PackageInstaller.STATUS_FAILURE_ABORTED -> "Se canceló la instalación"
        PackageInstaller.STATUS_FAILURE_BLOCKED -> "Android bloqueó la instalación"
        // Casi siempre es que la firma del APK no es la de la versión instalada.
        PackageInstaller.STATUS_FAILURE_INCOMPATIBLE ->
            "Android no acepta esta versión encima de la instalada. Suele pasar si la firma no coincide"
        PackageInstaller.STATUS_FAILURE_INVALID -> "El APK descargado no es válido"
        PackageInstaller.STATUS_FAILURE_STORAGE -> "No hay espacio para instalar la actualización"
        else -> "No se pudo instalar la actualización (código $status)"
    }
}
