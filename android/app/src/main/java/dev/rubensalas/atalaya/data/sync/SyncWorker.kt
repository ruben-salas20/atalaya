package dev.rubensalas.atalaya.data.sync

import android.content.Context
import androidx.work.BackoffPolicy
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import dev.rubensalas.atalaya.AtalayaApp
import dev.rubensalas.atalaya.data.api.ApiException
import java.util.concurrent.TimeUnit

/** Sincroniza con el servicio en segundo plano (S4). Solo corre con red. */
class SyncWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        val container = (applicationContext as AtalayaApp).container
        if (container.settings.current().apiKey.isBlank()) return Result.success()
        return try {
            container.offline.sync()
            Result.success()
        } catch (e: ApiException) {
            // Sin red o el servicio caído: WorkManager lo repite más tarde. Una clave mala no se arregla sola.
            if (e.status == null || e.status >= 500) Result.retry() else Result.failure()
        }
    }
}

/** Cuándo se sincroniza: cada 30 minutos, al abrir la app y al volver la red con cambios pendientes. */
object Sync {
    private const val PERIODIC = "sync-periodic"
    private const val NOW = "sync-now"

    private val online = Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build()

    fun schedulePeriodic(context: Context) {
        val request = PeriodicWorkRequestBuilder<SyncWorker>(30, TimeUnit.MINUTES)
            .setConstraints(online)
            .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, 1, TimeUnit.MINUTES)
            .build()
        WorkManager.getInstance(context).enqueueUniquePeriodicWork(PERIODIC, ExistingPeriodicWorkPolicy.KEEP, request)
    }

    /** Sincroniza en cuanto haya red. Si ya hay una esperando, se queda esa: también manda lo pendiente. */
    fun syncNow(context: Context) {
        val request = OneTimeWorkRequestBuilder<SyncWorker>()
            .setConstraints(online)
            .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, 30, TimeUnit.SECONDS)
            .build()
        WorkManager.getInstance(context).enqueueUniqueWork(NOW, ExistingWorkPolicy.KEEP, request)
    }
}
