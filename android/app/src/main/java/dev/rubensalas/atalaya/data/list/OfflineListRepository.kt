package dev.rubensalas.atalaya.data.list

import dev.rubensalas.atalaya.data.api.ApiException
import dev.rubensalas.atalaya.data.api.Cleanup
import dev.rubensalas.atalaya.data.api.AtalayaClient
import dev.rubensalas.atalaya.data.api.Entry
import dev.rubensalas.atalaya.data.api.EntryPage
import dev.rubensalas.atalaya.data.api.EntryQuery
import dev.rubensalas.atalaya.data.api.EntrySummary
import dev.rubensalas.atalaya.data.api.EntryUpdate
import dev.rubensalas.atalaya.data.api.Id
import dev.rubensalas.atalaya.data.api.MarkAllReadQuery
import dev.rubensalas.atalaya.data.api.Sidebar
import dev.rubensalas.atalaya.data.api.TranslationState
import dev.rubensalas.atalaya.data.api.View
import dev.rubensalas.atalaya.data.local.LocalContent
import dev.rubensalas.atalaya.data.local.LocalDao
import dev.rubensalas.atalaya.data.local.LocalEntry
import dev.rubensalas.atalaya.data.local.LocalSidebar
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import java.time.Duration
import java.time.Instant

/**
 * Lista y artículos con copia en el móvil (L8, S4).
 *
 * Qué se guarda (ADR 0003): el menú lateral, las filas de los sin leer (los
 * 200 más nuevos) y de todos los guardados, y el texto completo solo de los
 * guardados y de los artículos ya abiertos. Nunca pide el texto de un sin leer
 * que Rubén no abrió: esa llamada pone a traducir la GPU.
 *
 * Un cambio de leído o guardado se apunta primero en el móvil. Si no llega al
 * servicio, se queda marcado como pendiente y se manda al volver la red; hasta
 * entonces, ese valor manda sobre lo que diga Miniflux.
 */
class OfflineListRepository(
    private val client: AtalayaClient,
    private val dao: LocalDao,
    private val onSynced: suspend () -> Unit,
    /** Pide mandar los cambios pendientes en cuanto haya red. */
    private val requestPush: () -> Unit,
    private val now: () -> Instant = Instant::now,
) : ListRepository {
    /** Protege leer y escribir la base en un solo paso. Nunca se tiene durante una llamada a la red. */
    private val lock = Mutex()
    private val pushLock = Mutex()

    override suspend fun sidebar(): Sidebar {
        try {
            pushPending()
        } catch (_: ApiException) {
            // Sin red: la llamada de abajo dará el mismo fallo.
        }
        val sidebar = client.sidebar()
        lock.withLock { saveSidebar(sidebar) }
        return sidebar
    }

    override suspend fun entries(query: EntryQuery): EntryPage {
        val page = client.entries(query)
        // Un resultado de búsqueda no se guarda: no es una lista que haya que tener sin red.
        if (searchNeedle(query.q) != null) return page
        val merged = lock.withLock { mergeAndStore(page.entries, keepAll = false) }
        return EntryPage(page.total, merged)
    }

    override suspend fun cachedSidebar(): Sidebar? =
        dao.sidebar()?.let { client.json.decodeFromString(Sidebar.serializer(), it.json) }

    override suspend fun cachedEntries(query: EntryQuery): EntryPage? {
        val sidebar = cachedSidebar()
        val rows = dao.entries()
        if (sidebar == null && rows.isEmpty()) return null
        val feeds = feedsIn(sidebar, query.feedId, query.categoryId)
        // Como el servicio: busca en lo guardado de la vista, sin importar tildes ni mayúsculas.
        val needle = searchNeedle(query.q)
        val matching = rows
            .filter { feeds == null || it.feedId in feeds }
            .filter {
                when (query.view) {
                    View.All -> true
                    View.Unread -> !it.read
                    View.Starred -> it.starred
                }
            }
            .filter { needle == null || titleMatches(needle, it.title, it.originalTitle) }
            .sortedWith(compareByDescending<LocalEntry> { it.publishedAt }.thenByDescending { it.id })
        val offset = query.offset ?: 0
        val limit = query.limit ?: DEFAULT_LIMIT
        return EntryPage(matching.size.toLong(), matching.drop(offset).take(limit).map { it.toSummary() })
    }

    override suspend fun entry(id: Id): Entry {
        val row = dao.entry(id)
        val stored = dao.content(id)
        if (row != null && stored != null && stored.translationState in FINAL) {
            if (!stored.opened) dao.upsertContent(stored.copy(opened = true))
            return stored.toEntry(row)
        }
        val fresh = try {
            client.entry(id)
        } catch (e: ApiException) {
            if (e.status != null) throw e
            if (row != null && stored != null) return stored.toEntry(row)
            throw ApiException(null, NOT_SAVED, e)
        }
        return lock.withLock { storeEntry(fresh, opened = true) }
    }

    override suspend fun updateEntry(id: Id, update: EntryUpdate): EntrySummary {
        val before = lock.withLock {
            val row = dao.entry(id) ?: return@withLock null
            setFlags(
                row,
                read = update.read ?: row.read,
                starred = update.starred ?: row.starred,
                readDirty = row.readDirty || update.read != null,
                starredDirty = row.starredDirty || update.starred != null,
            )
            row
        }
        if (before == null) {
            // No está en el móvil: sin copia que enseñar, solo vale la red.
            val after = client.updateEntry(id, update)
            lock.withLock { mergeAndStore(listOf(after), keepAll = false) }
            return after
        }
        try {
            val after = client.updateEntry(id, update)
            // Guardar pone a traducir el texto (T2): una sincronización lo baja al móvil para leerlo sin red.
            if (update.starred == true) requestPush()
            return lock.withLock { confirm(id, update, after) } ?: after
        } catch (e: ApiException) {
            if (retryLater(e)) {
                requestPush()
                return (dao.entry(id) ?: before).toSummary()
            }
            lock.withLock {
                dao.entry(id)?.let { setFlags(it, before.read, before.starred, before.readDirty, before.starredDirty) }
            }
            throw e
        }
    }

    override suspend fun markAllRead(scope: MarkAllReadQuery) {
        client.markAllRead(scope)
        lock.withLock {
            val feeds = feedsIn(cachedSidebar(), scope.feedId, scope.categoryId)
            val seenAt = now().toEpochMilli()
            // «Todos» local incluye lo guardado de papers, que el servicio no marca (ver la issue #19).
            val changed = dao.entries()
                .filter { !it.read && (feeds == null || it.feedId in feeds) }
                .map { it.copy(read = true, readSeenAt = it.readSeenAt ?: seenAt) }
            if (changed.isEmpty()) return@withLock
            dao.upsertEntries(changed)
            cachedSidebar()?.let { sidebar ->
                val adjusted = changed.groupingBy { it.feedId }.eachCount().entries.fold(sidebar) { s, (feed, n) ->
                    s.adjusted(feed, -n, 0)
                }
                saveSidebar(adjusted)
            }
        }
    }

    override suspend fun markSynced() = onSynced()

    override suspend fun deleteTranslation(id: Id) {
        client.deleteEntryTranslation(id)
        // Sin la copia, al abrirlo se pide otra vez al servicio, que ya la traduce de nuevo (#91).
        lock.withLock { dao.deleteContents(listOf(id)) }
    }

    override suspend fun translationCleanupPreview(): Cleanup = client.translationCleanupPreview()

    override suspend fun cleanupTranslations(): Cleanup {
        val done = client.cleanupTranslations()
        // Lo mismo que en el servicio: el texto traducido de lo leído y no guardado sale también del móvil (#91).
        lock.withLock {
            val read = dao.entries().filter { it.read && !it.starred }.map { it.id }
            if (read.isNotEmpty()) dao.deleteContents(read)
        }
        return done
    }

    /**
     * Manda al servicio los cambios de leído y guardado apuntados sin red.
     * Lanza [ApiException] si sigue sin haber red, para volver a probar luego.
     */
    suspend fun pushPending() = pushLock.withLock {
        for (row in dao.dirtyEntries()) {
            val update = EntryUpdate(
                read = row.read.takeIf { row.readDirty },
                starred = row.starred.takeIf { row.starredDirty },
            )
            try {
                val after = client.updateEntry(row.id, update)
                lock.withLock { confirm(row.id, update, after) }
            } catch (e: ApiException) {
                if (retryLater(e)) throw e
                // El servicio no lo acepta (por ejemplo, el artículo ya no existe): se descarta.
                lock.withLock {
                    if (e.status == 404) {
                        dao.deleteEntries(listOf(row.id))
                        dao.deleteContents(listOf(row.id))
                    } else {
                        dao.entry(row.id)?.let { dao.upsertEntries(listOf(it.copy(readDirty = false, starredDirty = false))) }
                    }
                }
            }
        }
    }

    /**
     * Sincronización completa (S4): manda lo pendiente, baja el menú, los sin
     * leer y los guardados, el texto de los guardados, y borra lo viejo.
     * Devuelve si algún guardado sigue traduciéndose, para volver a por él.
     */
    suspend fun sync(): Boolean {
        pushPending()
        val sidebar = client.sidebar()
        val unread = client.entries(EntryQuery(view = View.Unread, limit = MAX_UNREAD))
        val starred = allStarred()
        lock.withLock {
            saveSidebar(sidebar)
            mergeAndStore((unread.entries + starred).distinctBy { it.id }, keepAll = false)
            reconcile(unread, starred.mapTo(HashSet()) { it.id })
            cleanUp(unread)
        }
        val stillTranslating = fetchStarredContents()
        onSynced()
        return stillTranslating
    }

    // -----------------------------------------------------------------------

    private suspend fun allStarred(): List<EntrySummary> {
        val all = ArrayList<EntrySummary>()
        while (true) {
            val page = client.entries(EntryQuery(view = View.Starred, limit = MAX_PAGE, offset = all.size))
            all += page.entries
            if (page.entries.isEmpty() || all.size >= page.total) return all
        }
    }

    /** Baja el texto de los guardados que aún no lo tienen, o cuya traducción no había terminado. */
    private suspend fun fetchStarredContents(): Boolean {
        val have = dao.contentInfo().associateBy { it.id }
        val missing = dao.entries().filter { row ->
            val info = have[row.id]
            row.starred && (info == null || info.translation == TranslationState.Pending.name)
        }
        for (row in missing) {
            val fresh = try {
                client.entry(row.id)
            } catch (e: ApiException) {
                if (retryLater(e)) throw e
                continue
            }
            lock.withLock { storeEntry(fresh, opened = false) }
        }
        val pending = TranslationState.Pending.name
        return dao.contentInfo().any { info -> info.translation == pending && missing.any { it.id == info.id } }
    }

    /**
     * Ajusta lo que cambió en Miniflux por otro lado: un sin leer que ya no
     * está entre los sin leer se leyó, y un guardado que ya no está se quitó.
     * Lo pendiente de enviar no se toca.
     */
    private suspend fun reconcile(unread: EntryPage, starredIds: Set<Id>) {
        val unreadIds = unread.entries.mapTo(HashSet()) { it.id }
        val complete = unread.entries.size >= unread.total
        val oldest = unread.entries.minOfOrNull { it.publishedAt.toEpochMilli() }
        val t = now().toEpochMilli()
        val changed = dao.entries().mapNotNull { row ->
            var r = row
            if (!r.starredDirty && r.starred && r.id !in starredIds) r = r.copy(starred = false)
            // Con la misma hora que el último de la ventana puede haber más sin leer fuera de ella.
            val inWindow = complete || (oldest != null && r.publishedAt > oldest)
            if (!r.readDirty && !r.read && r.id !in unreadIds && inWindow) r = r.copy(read = true, readSeenAt = t)
            r.takeIf { it != row }
        }
        if (changed.isNotEmpty()) dao.upsertEntries(changed)
    }

    /**
     * Borra lo que ya no hace falta: los leídos hace más de 7 días y los sin
     * leer que quedaron fuera de los 200 más nuevos, salvo guardados o
     * pendientes de enviar. Y el texto de lo que ni se abrió ni está guardado.
     */
    private suspend fun cleanUp(unread: EntryPage) {
        val cutoff = now().minus(KEEP_READ).toEpochMilli()
        val unreadIds = unread.entries.mapTo(HashSet()) { it.id }
        val complete = unread.entries.size >= unread.total
        val oldest = unread.entries.minOfOrNull { it.publishedAt.toEpochMilli() }
        val rows = dao.entries()
        val gone = rows.filter { r ->
            if (r.starred || r.dirty) return@filter false
            val oldRead = r.read && (r.readSeenAt ?: 0) < cutoff
            val outOfWindow = !r.read && !complete && r.id !in unreadIds && oldest != null && r.publishedAt < oldest
            oldRead || outOfWindow
        }.mapTo(HashSet()) { it.id }
        if (gone.isNotEmpty()) dao.deleteEntries(gone.toList())
        val kept = rows.filter { it.id !in gone }.associateBy { it.id }
        val orphanContents = dao.contentInfo().filter { info ->
            val row = kept[info.id]
            row == null || (!info.opened && !row.starred)
        }.map { it.id }
        if (orphanContents.isNotEmpty()) dao.deleteContents(orphanContents)
    }

    /** Los feeds que cubre un alcance, o null si cubre todo («Todos»). */
    private fun feedsIn(sidebar: Sidebar?, feedId: Id?, categoryId: Id?): Set<Id>? = when {
        feedId != null -> setOf(feedId)
        categoryId != null ->
            sidebar?.categories?.firstOrNull { it.id == categoryId }?.feeds?.mapTo(HashSet()) { it.id }.orEmpty()
        else -> null
    }

    private suspend fun saveSidebar(sidebar: Sidebar) =
        dao.saveSidebar(LocalSidebar(json = client.json.encodeToString(Sidebar.serializer(), sidebar)))

    /**
     * Junta lo que llegó del servicio con lo del móvil y lo guarda. Lo
     * pendiente de enviar manda. Sin [keepAll], solo guarda filas nuevas si
     * están sin leer o guardadas. Devuelve las filas tal como se deben ver.
     */
    private suspend fun mergeAndStore(remote: List<EntrySummary>, keepAll: Boolean): List<EntrySummary> {
        if (remote.isEmpty()) return emptyList()
        val local = dao.entries(remote.map { it.id }).associateBy { it.id }
        val t = now().toEpochMilli()
        val save = ArrayList<LocalEntry>()
        val merged = remote.map { r ->
            val l = local[r.id]
            val read = if (l?.readDirty == true) l.read else r.read
            val starred = if (l?.starredDirty == true) l.starred else r.starred
            val row = LocalEntry(
                id = r.id, feedId = r.feedId, feedTitle = r.feedTitle, title = r.title,
                originalTitle = r.originalTitle, titleTranslated = r.titleTranslated, lang = r.lang,
                url = r.url, publishedAt = r.publishedAt.toEpochMilli(), read = read, starred = starred,
                readSeenAt = if (read) l?.readSeenAt ?: t else null,
                readDirty = l?.readDirty ?: false,
                starredDirty = l?.starredDirty ?: false,
            )
            if (keepAll || l != null || !read || starred) save += row
            row.toSummary()
        }
        dao.upsertEntries(save)
        return merged
    }

    private suspend fun storeEntry(fresh: Entry, opened: Boolean): Entry {
        val row = mergeAndStore(listOf(fresh.summary), keepAll = true).single()
        val wasOpened = dao.content(fresh.id)?.opened ?: false
        dao.upsertContent(
            LocalContent(
                id = fresh.id, author = fresh.author, content = fresh.content,
                translation = fresh.translation.name,
                // Lo parcial (Pending) es de paso: solo se guarda la traducción completa (ADR 0015).
                translatedContent = fresh.translatedContent.takeIf { fresh.translation == TranslationState.Done },
                opened = opened || wasOpened,
            ),
        )
        return fresh.copy(read = row.read, starred = row.starred)
    }

    /** El servicio aceptó [sent]. Quita la marca de pendiente si nada cambió desde entonces. */
    private suspend fun confirm(id: Id, sent: EntryUpdate, after: EntrySummary): EntrySummary? {
        val row = dao.entry(id) ?: return null
        val readDirty = row.readDirty && !(sent.read != null && row.read == sent.read)
        val starredDirty = row.starredDirty && !(sent.starred != null && row.starred == sent.starred)
        return setFlags(
            row,
            read = if (readDirty) row.read else after.read,
            starred = if (starredDirty) row.starred else after.starred,
            readDirty = readDirty,
            starredDirty = starredDirty,
        ).toSummary()
    }

    /** Cambia leído y guardado de una fila y ajusta los números del menú guardado. */
    private suspend fun setFlags(row: LocalEntry, read: Boolean, starred: Boolean, readDirty: Boolean, starredDirty: Boolean): LocalEntry {
        val updated = row.copy(
            read = read,
            starred = starred,
            readSeenAt = if (read) row.readSeenAt ?: now().toEpochMilli() else null,
            readDirty = readDirty,
            starredDirty = starredDirty,
        )
        if (updated != row) dao.upsertEntries(listOf(updated))
        val unreadDelta = (if (read) 0 else 1) - (if (row.read) 0 else 1)
        val starredDelta = (if (starred) 1 else 0) - (if (row.starred) 1 else 0)
        if (unreadDelta != 0 || starredDelta != 0) {
            cachedSidebar()?.let { saveSidebar(it.adjusted(row.feedId, unreadDelta, starredDelta)) }
        }
        return updated
    }

    /** Sin respuesta o el servicio caído: el cambio se queda pendiente. */
    private fun retryLater(e: ApiException) = e.status == null || e.status >= 500

    companion object {
        const val MAX_UNREAD = 200
        const val MAX_PAGE = 200
        const val DEFAULT_LIMIT = 50
        val KEEP_READ: Duration = Duration.ofDays(7)
        val FINAL = setOf(TranslationState.Done, TranslationState.NotNeeded)
        const val NOT_SAVED = "Este artículo no está guardado en el móvil. Ábrelo cuando tengas conexión."
    }
}
