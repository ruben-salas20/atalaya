package dev.rubensalas.atalaya.data.list

import dev.rubensalas.atalaya.data.api.AtalayaClient
import dev.rubensalas.atalaya.data.api.Entry
import dev.rubensalas.atalaya.data.api.EntryPage
import dev.rubensalas.atalaya.data.api.EntryQuery
import dev.rubensalas.atalaya.data.api.EntrySummary
import dev.rubensalas.atalaya.data.api.EntryUpdate
import dev.rubensalas.atalaya.data.api.Id
import dev.rubensalas.atalaya.data.api.Sidebar
import dev.rubensalas.atalaya.data.settings.SettingsRepository

/**
 * De dónde sacan la lista y el artículo sus datos. La app usa
 * [OfflineListRepository], que guarda lo descargado en el móvil (L8).
 */
interface ListRepository {
    /** Del servicio. Sin red, lanza [dev.rubensalas.atalaya.data.api.ApiException] con `status` null. */
    suspend fun sidebar(): Sidebar
    suspend fun entries(query: EntryQuery): EntryPage

    /** El menú guardado en el móvil, o null si nunca se guardó. */
    suspend fun cachedSidebar(): Sidebar? = null

    /** La lista guardada en el móvil, o null si no hay nada guardado. */
    suspend fun cachedEntries(query: EntryQuery): EntryPage? = null

    /** Un artículo entero. Pedirlo pone en marcha su traducción (ADR 0003). */
    suspend fun entry(id: Id): Entry

    /**
     * Marca como leído o guarda (L4). Devuelve cómo queda el artículo. Sin
     * red, [OfflineListRepository] lo apunta y lo manda al volver la red.
     */
    suspend fun updateEntry(id: Id, update: EntryUpdate): EntrySummary

    /** Se llama cuando la lista y el menú llegaron bien (S2). */
    suspend fun markSynced()
}

/** Solo el servicio, sin guardar nada. Útil para probar sin Room. */
class RemoteListRepository(
    private val client: AtalayaClient,
    private val settings: SettingsRepository,
) : ListRepository {
    override suspend fun sidebar(): Sidebar = client.sidebar()
    override suspend fun entries(query: EntryQuery): EntryPage = client.entries(query)
    override suspend fun entry(id: Id): Entry = client.entry(id)
    override suspend fun updateEntry(id: Id, update: EntryUpdate): EntrySummary = client.updateEntry(id, update)
    override suspend fun markSynced() {
        settings.markSynced()
    }
}
