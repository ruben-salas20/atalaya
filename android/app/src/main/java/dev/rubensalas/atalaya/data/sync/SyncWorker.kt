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
import androidx.work.workDataOf
import dev.rubensalas.atalaya.AtalayaApp
import dev.rubensalas.atalaya.data.api.ApiException
import java.util.concurrent.TimeUnit

/** Sincroniza con el servicio en segundo plano (S4). Solo corre con red. */
class SyncWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        val container = (applicationContext as AtalayaApp).container
        if (container.settings.current().apiKey.isBlank()) return Result.success()
        return try {
            val stillTranslating = container.offline.sync()
            val round = inputData.getInt(Sync.ROUND, 0)
            if (stillTranslating && round < Sync.MAX_ROUNDS) Sync.syncSoon(applicationContext, round + 1)
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
    private const val SOON = "sync-soon"
    const val ROUND = "round"

    /** Un guardado tarda en traducirse unos minutos como mucho: 10 vueltas de 1 minuto bastan. */
    const val MAX_ROUNDS = 10

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

    /** Vuelve en un minuto a por el texto de un guardado que aún se traducía. */
    fun syncSoon(context: Context, round: Int) {
        val request = OneTimeWorkRequestBuilder<SyncWorker>()
            .setConstraints(online)
            .setInitialDelay(1, TimeUnit.MINUTES)
            .setInputData(workDataOf(ROUND to round))
            .build()
        // APPEND: la llama el propio trabajo de SOON mientras corre; REPLACE lo cancelaría.
        WorkManager.getInstance(context).enqueueUniqueWork(SOON, ExistingWorkPolicy.APPEND_OR_REPLACE, request)
    }
}
