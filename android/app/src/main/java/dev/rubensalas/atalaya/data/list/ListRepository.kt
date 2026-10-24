package dev.rubensalas.atalaya.data.list

import dev.rubensalas.atalaya.data.api.Entry
import dev.rubensalas.atalaya.data.api.EntryPage
import dev.rubensalas.atalaya.data.api.EntryQuery
import dev.rubensalas.atalaya.data.api.EntrySummary
import dev.rubensalas.atalaya.data.api.EntryUpdate
import dev.rubensalas.atalaya.data.api.Id
import dev.rubensalas.atalaya.data.api.MarkAllReadQuery
import dev.rubensalas.atalaya.data.api.Sidebar

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

    /**
     * Marca como leído todo lo del alcance. Solo funciona con red: no se
     * guarda para después, para no marcar algo que llegó tras la orden.
     */
    suspend fun markAllRead(scope: MarkAllReadQuery)

    /** Se llama cuando la lista y el menú llegaron bien (S2). */
    suspend fun markSynced()
}

