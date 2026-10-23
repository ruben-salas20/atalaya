package dev.rubensalas.atalaya.data.settings

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.floatPreferencesKey
import androidx.datastore.preferences.core.longPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import dev.rubensalas.atalaya.data.api.ServerConfig
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import java.time.Instant

/** Qué hace deslizar una fila de la lista hacia un lado (L4, S3). */
enum class SwipeAction(val label: String) {
    MarkRead("Marcar como leído"),
    Star("Guardar"),
    None("Nada"),
}

data class Settings(
    val serviceUrl: String = DEFAULT_SERVICE_URL,
    val apiKey: String = "",
    /** Multiplica el tamaño de toda la letra de la app. */
    val fontScale: Float = 1f,
    val swipeLeft: SwipeAction = SwipeAction.MarkRead,
    val swipeRight: SwipeAction = SwipeAction.Star,
    /** Última vez que «Probar» o una sincronización llegaron al servicio. */
    val lastConnectedAt: Instant? = null,
    /** Última sincronización completa con el servicio (S2). */
    val lastSyncAt: Instant? = null,
) {
    val serverConfig get() = ServerConfig(serviceUrl, apiKey)

    companion object {
        const val DEFAULT_SERVICE_URL = "https://atalaya.rubensalas.dev"
        const val MIN_FONT_SCALE = 0.85f
        const val MAX_FONT_SCALE = 1.3f
    }
}

private val Context.dataStore: DataStore<Preferences> by preferencesDataStore(name = "settings")

class SettingsRepository(context: Context) {
    private val store = context.applicationContext.dataStore

    val settings: Flow<Settings> = store.data.map { p ->
        Settings(
            serviceUrl = p[SERVICE_URL] ?: Settings.DEFAULT_SERVICE_URL,
            apiKey = p[API_KEY] ?: "",
            fontScale = p[FONT_SCALE] ?: 1f,
            swipeLeft = p[SWIPE_LEFT]?.let(::swipeAction) ?: SwipeAction.MarkRead,
            swipeRight = p[SWIPE_RIGHT]?.let(::swipeAction) ?: SwipeAction.Star,
            lastConnectedAt = p[LAST_CONNECTED_AT]?.let(Instant::ofEpochMilli),
            lastSyncAt = p[LAST_SYNC_AT]?.let(Instant::ofEpochMilli),
        )
    }

    suspend fun current(): Settings = settings.first()

    suspend fun setServiceUrl(value: String) = store.edit { it[SERVICE_URL] = value.trim() }
    suspend fun setApiKey(value: String) = store.edit { it[API_KEY] = value.trim() }
    suspend fun setFontScale(value: Float) = store.edit {
        it[FONT_SCALE] = value.coerceIn(Settings.MIN_FONT_SCALE, Settings.MAX_FONT_SCALE)
    }
    suspend fun setSwipeLeft(value: SwipeAction) = store.edit { it[SWIPE_LEFT] = value.name }
    suspend fun setSwipeRight(value: SwipeAction) = store.edit { it[SWIPE_RIGHT] = value.name }
    suspend fun markConnected(at: Instant = Instant.now()) = store.edit { it[LAST_CONNECTED_AT] = at.toEpochMilli() }
    suspend fun markSynced(at: Instant = Instant.now()) = store.edit {
        it[LAST_SYNC_AT] = at.toEpochMilli()
        it[LAST_CONNECTED_AT] = at.toEpochMilli()
    }

    private fun swipeAction(name: String) = SwipeAction.entries.firstOrNull { it.name == name }

    private companion object {
        val SERVICE_URL = stringPreferencesKey("service_url")
        val API_KEY = stringPreferencesKey("api_key")
        val FONT_SCALE = floatPreferencesKey("font_scale")
        val SWIPE_LEFT = stringPreferencesKey("swipe_left")
        val SWIPE_RIGHT = stringPreferencesKey("swipe_right")
        val LAST_CONNECTED_AT = longPreferencesKey("last_connected_at")
        val LAST_SYNC_AT = longPreferencesKey("last_sync_at")
    }
}
