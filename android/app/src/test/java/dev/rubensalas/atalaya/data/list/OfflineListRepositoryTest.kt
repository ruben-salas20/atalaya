package dev.rubensalas.atalaya.data.list

import dev.rubensalas.atalaya.data.api.ApiException
import dev.rubensalas.atalaya.data.api.AtalayaClient
import dev.rubensalas.atalaya.data.api.Entry
import dev.rubensalas.atalaya.data.api.EntryPage
import dev.rubensalas.atalaya.data.api.EntryQuery
import dev.rubensalas.atalaya.data.api.EntrySummary
import dev.rubensalas.atalaya.data.api.EntryUpdate
import dev.rubensalas.atalaya.data.api.Id
import dev.rubensalas.atalaya.data.api.Progress
import dev.rubensalas.atalaya.data.api.ServerConfig
import dev.rubensalas.atalaya.data.api.Sidebar
import dev.rubensalas.atalaya.data.api.SidebarCategory
import dev.rubensalas.atalaya.data.api.SidebarFeed
import dev.rubensalas.atalaya.data.api.TranslationState
import dev.rubensalas.atalaya.data.api.View
import dev.rubensalas.atalaya.data.local.ContentInfo
import dev.rubensalas.atalaya.data.local.LocalContent
import dev.rubensalas.atalaya.data.local.LocalDao
import dev.rubensalas.atalaya.data.local.LocalEntry
import dev.rubensalas.atalaya.data.local.LocalSidebar
import kotlinx.coroutines.test.runTest
import mockwebserver3.Dispatcher
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import mockwebserver3.RecordedRequest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.time.Duration
import java.time.Instant

class OfflineListRepositoryTest {
    /** Un servicio de mentira con su estado, como Miniflux. */
    private class FakeService(private val client: () -> AtalayaClient) : Dispatcher() {
        val entries = linkedMapOf<Id, Entry>()
        val requests = mutableListOf<String>()

        override fun dispatch(request: RecordedRequest): MockResponse = synchronized(this) {
            val path = request.url.encodedPath
            requests += "${request.method} $path"
            val json = client().json
            when {
                path == "/api/sidebar" -> {
                    val unread = entries.values.count { !it.read }.toLong()
                    val sidebar = Sidebar(
                        total = entries.size.toLong(), unread = unread,
                        starred = entries.values.count { it.starred }.toLong(),
                        categories = listOf(SidebarCategory(1, "Tecnología", unread, listOf(SidebarFeed(2, "The Verge", unread)))),
                    )
                    MockResponse(body = json.encodeToString(Sidebar.serializer(), sidebar))
                }
                path == "/api/entries" -> {
                    val view = request.url.queryParameter("view")
                    val limit = request.url.queryParameter("limit")?.toInt() ?: 50
                    val offset = request.url.queryParameter("offset")?.toInt() ?: 0
                    val all = entries.values
                        .filter { view != "unread" || !it.read }
                        .filter { view != "starred" || it.starred }
                        .sortedByDescending { it.publishedAt }
                        .map { it.summary }
                    val page = EntryPage(all.size.toLong(), all.drop(offset).take(limit))
                    MockResponse(body = json.encodeToString(EntryPage.serializer(), page))
                }
                path.startsWith("/api/entries/") -> {
                    val id = path.substringAfterLast('/').toLong()
                    val e = entries[id] ?: return MockResponse(code = 404, body = """{"error":"no existe"}""")
                    if (request.method == "PATCH") {
                        val update = json.decodeFromString(EntryUpdate.serializer(), request.body!!.utf8())
                        val after = e.copy(read = update.read ?: e.read, starred = update.starred ?: e.starred)
                        entries[id] = after
                        MockResponse(body = json.encodeToString(EntrySummary.serializer(), after.summary))
                    } else {
                        MockResponse(body = json.encodeToString(Entry.serializer(), e))
                    }
                }
                else -> MockResponse(code = 404)
            }
        }
    }

    private class FakeDao : LocalDao {
        val rows = linkedMapOf<Id, LocalEntry>()
        val contents = linkedMapOf<Id, LocalContent>()
        var side: LocalSidebar? = null

        override suspend fun entries() = rows.values.toList()
        override suspend fun entry(id: Id) = rows[id]
        override suspend fun entries(ids: List<Id>) = ids.mapNotNull { rows[it] }
        override suspend fun dirtyEntries() = rows.values.filter { it.dirty }
        override suspend fun upsertEntries(entries: List<LocalEntry>) = entries.forEach { rows[it.id] = it }
        override suspend fun deleteEntries(ids: List<Id>) = ids.forEach { rows.remove(it) }
        override suspend fun content(id: Id) = contents[id]
        override suspend fun contentInfo() = contents.values.map { ContentInfo(it.id, it.translation, it.opened) }
        override suspend fun upsertContent(content: LocalContent) {
            contents[content.id] = content
        }
        override suspend fun deleteContents(ids: List<Id>) = ids.forEach { contents.remove(it) }
        override suspend fun sidebar() = side
        override suspend fun saveSidebar(sidebar: LocalSidebar) {
            side = sidebar
        }
    }

    private val server = MockWebServer()
    private val dao = FakeDao()
    private var online = true
    private var clock = Instant.parse("2026-10-23T12:00:00Z")
    private var synced = 0
    private var pushRequests = 0
    private lateinit var client: AtalayaClient
    private val service = FakeService { client }
    private lateinit var repo: OfflineListRepository

    @Before
    fun setUp() {
        server.dispatcher = service
        server.start()
        // Sin red: un puerto donde nadie escucha da el mismo fallo que el modo avión.
        client = AtalayaClient(config = {
            ServerConfig(if (online) server.url("/").toString() else "http://127.0.0.1:9/", "clave")
        })
        repo = OfflineListRepository(
            client, dao,
            onSynced = { synced++ },
            requestPush = { pushRequests++ },
            now = { clock },
        )
        service.entries[1] = entry(1, hour = 11)
        service.entries[2] = entry(2, hour = 10, read = true, starred = true)
        service.entries[3] = entry(3, hour = 9, read = true)
    }

    @After
    fun tearDown() = server.close()

    @Test
    fun starringOnlineFetchesTextAndRetriesWhileTranslating() = runTest {
        repo.sync()
        pushRequests = 0
        service.entries[1] = service.entries[1]!!.copy(translation = TranslationState.Pending, translatedContent = null)
        repo.updateEntry(1, EntryUpdate(starred = true))
        assertEquals(1, pushRequests)
        assertTrue(repo.sync())
        assertEquals(TranslationState.Pending.name, dao.contents[1]!!.translation)

        service.entries[1] = service.entries[1]!!.copy(translation = TranslationState.Done, translatedContent = "<p>Hola</p>")
        assertFalse(repo.sync())
        assertEquals("<p>Hola</p>", dao.contents[1]!!.translatedContent)
    }

    @Test
    fun aPartialTranslationIsShownButNotStored() = runTest {
        val partial = "<p>Hola</p><p><span class=\"atalaya-pendiente\">Two.</span></p>"
        service.entries[1] = service.entries[1]!!.copy(
            translation = TranslationState.Pending, translatedContent = partial, progress = Progress(1, 2),
        )
        val shown = repo.entry(1)
        assertEquals(partial, shown.translatedContent)
        assertEquals(Progress(1, 2), shown.progress)
        // Room guarda que sigue traduciéndose, pero no lo parcial: solo la traducción completa (ADR 0015).
        assertEquals(TranslationState.Pending.name, dao.contents[1]!!.translation)
        assertNull(dao.contents[1]!!.translatedContent)

        service.entries[1] = service.entries[1]!!.copy(
            translation = TranslationState.Done, translatedContent = "<p>Hola</p><p>Dos.</p>", progress = null,
        )
        repo.entry(1)
        assertEquals("<p>Hola</p><p>Dos.</p>", dao.contents[1]!!.translatedContent)
    }

    @Test
    fun syncKeepsUnreadAndStarredAndOnlyTheTextOfStarred() = runTest {
        repo.sync()
        assertEquals(setOf(1L, 2L), dao.rows.keys)
        assertEquals(setOf(2L), dao.contents.keys)
        assertFalse("nunca se pide el texto de un sin leer", "GET /api/entries/1" in service.requests)
        assertEquals(1, synced)
        assertNotNull(dao.side)
    }

    @Test
    fun withoutNetworkTheListComesFromThePhone() = runTest {
        repo.sync()
        online = false
        val e = runCatching { repo.entries(EntryQuery(view = View.Unread)) }.exceptionOrNull() as ApiException
        assertNull(e.status)
        assertEquals(listOf(1L), repo.cachedEntries(EntryQuery(view = View.Unread))!!.entries.map { it.id })
        assertEquals(listOf(2L), repo.cachedEntries(EntryQuery(view = View.Starred))!!.entries.map { it.id })
        assertEquals(listOf(1L, 2L), repo.cachedEntries(EntryQuery(view = View.All, categoryId = 1))!!.entries.map { it.id })
        assertEquals(1L, repo.cachedSidebar()!!.unread)
    }

    @Test
    fun nothingSavedMeansNoCache() = runTest {
        assertNull(repo.cachedEntries(EntryQuery()))
    }

    @Test
    fun offlineSearchFiltersTheSavedListByTitle() = runTest {
        repo.sync()
        online = false
        assertEquals(listOf(1L), repo.cachedEntries(EntryQuery(view = View.Unread, q = " T1 "))!!.entries.map { it.id })
        assertEquals(emptyList<Long>(), repo.cachedEntries(EntryQuery(view = View.Unread, q = "zzz"))!!.entries.map { it.id })
    }

    @Test
    fun savedAndOpenedArticlesReadOffline() = runTest {
        repo.sync()
        repo.entry(3)
        online = false
        assertEquals("<p>2</p>", repo.entry(2).content)
        assertEquals("<p>3</p>", repo.entry(3).content)
        service.entries[4] = entry(4, hour = 8)
        val e = runCatching { repo.entry(4) }.exceptionOrNull() as ApiException
        assertEquals(OfflineListRepository.NOT_SAVED, e.message)
    }

    @Test
    fun finishedTranslationIsNotAskedAgain() = runTest {
        repo.sync()
        service.requests.clear()
        repo.entry(2)
        assertTrue(service.requests.isEmpty())
    }

    @Test
    fun offlineSwipeIsKeptAndSentWhenTheNetworkReturns() = runTest {
        repo.sync()
        online = false
        val after = repo.updateEntry(1, EntryUpdate(read = true))
        assertTrue(after.read)
        assertEquals(1, pushRequests)
        assertTrue(dao.rows[1]!!.readDirty)
        assertEquals(0L, repo.cachedSidebar()!!.unread)
        assertFalse(service.entries[1]!!.read)

        online = true
        repo.sync()
        assertTrue(service.entries[1]!!.read)
        assertFalse(dao.rows[1]!!.dirty)
    }

    @Test
    fun pendingChangeWinsOnlyForItsOwnField() = runTest {
        repo.sync()
        online = false
        repo.updateEntry(1, EntryUpdate(starred = true))
        // Mientras tanto, en Miniflux alguien lo lee.
        service.entries[1] = service.entries[1]!!.copy(read = true)
        online = true
        repo.sync()
        val server = service.entries[1]!!
        val local = dao.rows[1]!!
        assertTrue(server.read && server.starred)
        assertTrue(local.read && local.starred && !local.dirty)
    }

    @Test
    fun pendingValueSurvivesARefreshBeforeItIsSent() = runTest {
        repo.sync()
        online = false
        repo.updateEntry(1, EntryUpdate(read = true))
        // La red vuelve, pero el envío aún no ha salido: la lista no debe deshacer el gesto.
        online = true
        val page = repo.entries(EntryQuery(view = View.All))
        assertTrue(page.entries.first { it.id == 1L }.read)
    }

    @Test
    fun readElsewhereIsReadHereAndOldReadOnesAreDeleted() = runTest {
        repo.sync()
        service.entries[1] = service.entries[1]!!.copy(read = true)
        repo.sync()
        assertTrue(dao.rows[1]!!.read)

        clock = clock.plus(Duration.ofDays(8))
        repo.sync()
        assertEquals("el guardado se queda", setOf(2L), dao.rows.keys)
    }

    @Test
    fun unstarredElsewhereLosesItsText() = runTest {
        repo.sync()
        service.entries[2] = service.entries[2]!!.copy(starred = false)
        clock = clock.plus(Duration.ofDays(8))
        repo.sync()
        assertFalse(2L in dao.rows)
        assertTrue(dao.contents.isEmpty())
    }

    @Test
    fun rejectedChangeIsUndone() = runTest {
        repo.sync()
        service.entries.remove(1)
        val e = runCatching { repo.updateEntry(1, EntryUpdate(read = true)) }.exceptionOrNull() as ApiException
        assertEquals(404, e.status)
        assertFalse(dao.rows[1]!!.read)
        assertFalse(dao.rows[1]!!.dirty)
        assertEquals(1L, repo.cachedSidebar()!!.unread)
    }

    private companion object {
        fun entry(id: Long, hour: Int, read: Boolean = false, starred: Boolean = false) = Entry(
            id = id, feedId = 2, feedTitle = "The Verge", title = "T$id", originalTitle = "T$id",
            titleTranslated = true, lang = "en", url = "https://x/$id",
            publishedAt = Instant.parse("2026-10-23T%02d:00:00Z".format(hour)), read = read, starred = starred,
            content = "<p>$id</p>", translation = TranslationState.Done, translatedContent = "<p>es $id</p>",
        )
    }
}
