package dev.rubensalas.atalaya.data.list

import dev.rubensalas.atalaya.data.api.AtalayaClient
import dev.rubensalas.atalaya.data.api.EntryPage
import dev.rubensalas.atalaya.data.api.EntryQuery
import dev.rubensalas.atalaya.data.api.Sidebar
import dev.rubensalas.atalaya.data.settings.SettingsRepository

/**
 * De dónde saca la lista sus datos. Hoy, del servicio. El paso 3.5 la cambia
 * por una que guarda en Room y sirve sin conexión, sin tocar la pantalla.
 */
interface ListRepository {
    suspend fun sidebar(): Sidebar
    suspend fun entries(query: EntryQuery): EntryPage

    /** Se llama cuando la lista y el menú llegaron bien (S2). */
    suspend fun markSynced()
}

class RemoteListRepository(
    private val client: AtalayaClient,
    private val settings: SettingsRepository,
) : ListRepository {
    override suspend fun sidebar(): Sidebar = client.sidebar()
    override suspend fun entries(query: EntryQuery): EntryPage = client.entries(query)
    override suspend fun markSynced() {
        settings.markSynced()
    }
}
