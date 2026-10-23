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
 * De dónde sacan la lista y el artículo sus datos. Hoy, del servicio. El paso
 * 3.5 la cambia por una que guarda en Room y sirve sin conexión, sin tocar
 * las pantallas.
 */
interface ListRepository {
    suspend fun sidebar(): Sidebar
    suspend fun entries(query: EntryQuery): EntryPage

    /** Un artículo entero. Pedirlo pone en marcha su traducción (ADR 0003). */
    suspend fun entry(id: Id): Entry

    /** Marca como leído o guarda (L4). Devuelve cómo queda el artículo. */
    suspend fun updateEntry(id: Id, update: EntryUpdate): EntrySummary

    /** Se llama cuando la lista y el menú llegaron bien (S2). */
    suspend fun markSynced()
}

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
