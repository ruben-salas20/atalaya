package dev.rubensalas.atalaya.ui.list

import dev.rubensalas.atalaya.data.api.ApiException
import dev.rubensalas.atalaya.data.api.Entry
import dev.rubensalas.atalaya.data.api.EntryPage
import dev.rubensalas.atalaya.data.api.EntryQuery
import dev.rubensalas.atalaya.data.api.EntrySummary
import dev.rubensalas.atalaya.data.api.EntryUpdate
import dev.rubensalas.atalaya.data.api.Id
import dev.rubensalas.atalaya.data.api.Sidebar
import dev.rubensalas.atalaya.data.api.SidebarCategory
import dev.rubensalas.atalaya.data.api.SidebarFeed
import dev.rubensalas.atalaya.data.api.View
import dev.rubensalas.atalaya.data.list.ListRepository
import dev.rubensalas.atalaya.data.settings.SwipeAction
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.time.Instant

@OptIn(ExperimentalCoroutinesApi::class)
class ListViewModelTest {
    private class FakeRepo : ListRepository {
        val queries = mutableListOf<EntryQuery>()
        var synced = 0
        var fail: ApiException? = null
        var total = 120L

        override suspend fun sidebar() = fail?.let { throw it } ?: SIDEBAR

        override suspend fun entries(query: EntryQuery): EntryPage {
            fail?.let { throw it }
            queries += query
            val from = (query.offset ?: 0).toLong()
            val to = minOf(total, from + (query.limit ?: 50))
            return EntryPage(total, (from until to).map(::entry))
        }

        val updates = mutableListOf<Pair<Long, EntryUpdate>>()
        var failUpdate: ApiException? = null

        override suspend fun entry(id: Id): Entry = throw UnsupportedOperationException()

        override suspend fun updateEntry(id: Id, update: EntryUpdate): EntrySummary {
            failUpdate?.let { throw it }
            updates += id to update
            return Companion.entry(id).copy(read = update.read ?: false, starred = update.starred ?: false)
        }

        override suspend fun markSynced() {
            synced++
        }

        var cache: EntryPage? = null
        override suspend fun cachedSidebar() = cache?.let { SIDEBAR }
        override suspend fun cachedEntries(query: EntryQuery) = cache
    }

    private val repo = FakeRepo()
    private lateinit var vm: ListViewModel

    @Before
    fun setUp() {
        Dispatchers.setMain(UnconfinedTestDispatcher())
        vm = ListViewModel(repo)
    }

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun refreshLoadsUnreadAndMarksSynced() {
        vm.refresh()
        val s = vm.state.value
        assertEquals(50, s.entries.size)
        assertEquals(SIDEBAR, s.sidebar)
        assertEquals(7L, s.count)
        assertEquals(View.Unread, repo.queries.single().view)
        assertEquals(1, repo.synced)
        assertFalse(s.loading)
    }

    @Test
    fun loadMoreAppendsNextPage() {
        vm.refresh()
        vm.loadMore()
        vm.loadMore()
        vm.loadMore()
        assertEquals(120, vm.state.value.entries.size)
        assertEquals(listOf(0, 50, 100), repo.queries.map { it.offset })
        assertFalse(vm.state.value.canLoadMore)
    }

    @Test
    fun refreshKeepsWhatWasLoaded() {
        vm.refresh()
        vm.loadMore()
        vm.refresh()
        assertEquals(100, repo.queries.last().limit)
        assertEquals(100, vm.state.value.entries.size)
    }

    @Test
    fun selectingAFeedAsksForAllOfIt() {
        vm.refresh()
        vm.select(ListSelection.Feed(2, "The Verge"))
        val q = repo.queries.last()
        assertEquals(View.All, q.view)
        assertEquals(2L, q.feedId)
        assertEquals(3L, vm.state.value.count)
    }

    @Test
    fun wrongKeyPointsToSettings() {
        repo.fail = ApiException(401, "Unauthorized")
        vm.refresh()
        assertEquals("La clave no es correcta. Revísala en Ajustes.", vm.state.value.error)
        assertEquals(0, repo.synced)
        assertFalse(vm.state.value.loading)
        repo.fail = null
        vm.refresh()
        assertNull(vm.state.value.error)
    }

    @Test
    fun swipeLeftMarksReadWithoutReloading() {
        vm.refresh()
        val first = vm.state.value.entries[0]
        vm.swipe(first, SwipeAction.MarkRead)
        val s = vm.state.value
        assertTrue(s.entries[0].read)
        assertEquals(listOf(first.id to EntryUpdate(read = true)), repo.updates)
        assertEquals(1, repo.queries.size)
        assertEquals(6L, s.sidebar!!.unread)
        assertEquals(6L, s.sidebar!!.categories[0].unread)
        assertEquals(2L, s.sidebar!!.categories[0].feeds[0].unread)
        assertEquals(6L, s.count)
    }

    @Test
    fun swipeRightStarsAndTogglesBack() {
        vm.refresh()
        vm.swipe(vm.state.value.entries[3], SwipeAction.Star)
        assertTrue(vm.state.value.entries[3].starred)
        assertEquals(2L, vm.state.value.sidebar!!.starred)
        vm.swipe(vm.state.value.entries[3], SwipeAction.Star)
        assertFalse(vm.state.value.entries[3].starred)
        assertEquals(1L, vm.state.value.sidebar!!.starred)
        assertEquals(EntryUpdate(starred = false), repo.updates.last().second)
    }

    @Test
    fun failedSwipeIsUndone() {
        vm.refresh()
        repo.failUpdate = ApiException(null, "No se pudo conectar con el servicio")
        vm.swipe(vm.state.value.entries[0], SwipeAction.MarkRead)
        val s = vm.state.value
        assertFalse(s.entries[0].read)
        assertEquals(7L, s.sidebar!!.unread)
        assertEquals("No se pudo cambiar: No se pudo conectar con el servicio", s.error)
    }

    @Test
    fun swipeWithNoActionDoesNothing() {
        vm.refresh()
        vm.swipe(vm.state.value.entries[0], SwipeAction.None)
        assertTrue(repo.updates.isEmpty())
    }

    @Test
    fun withoutNetworkShowsWhatThePhoneKeptAndSaysSoOnce() {
        repo.fail = ApiException(null, "No se pudo conectar con el servicio")
        repo.cache = EntryPage(2, listOf(entry(5), entry(6)))
        vm.refresh()
        val s = vm.state.value
        assertEquals(listOf(5L, 6L), s.entries.map { it.id })
        assertTrue(s.offline)
        assertEquals(ListViewModel.OFFLINE_NOTICE, s.error)
        assertEquals(0, repo.synced)
        vm.dismissError()
        vm.refresh()
        assertNull("el aviso no se repite", vm.state.value.error)

        repo.fail = null
        vm.refresh()
        assertFalse(vm.state.value.offline)
        assertEquals(1, repo.synced)
    }

    @Test
    fun wrongKeyDoesNotFallBackToThePhone() {
        repo.fail = ApiException(401, "Unauthorized")
        repo.cache = EntryPage(1, listOf(entry(5)))
        vm.refresh()
        assertEquals("La clave no es correcta. Revísala en Ajustes.", vm.state.value.error)
        assertFalse(vm.state.value.offline)
    }

    private companion object {
        val SIDEBAR = Sidebar(
            total = 20, unread = 7, starred = 1,
            categories = listOf(SidebarCategory(1, "Tecnología", 7, listOf(SidebarFeed(2, "The Verge", 3)))),
        )

        fun entry(id: Long) = EntrySummary(
            id = id, feedId = 2, feedTitle = "The Verge", title = "T$id", originalTitle = "T$id",
            titleTranslated = true, lang = "en", url = "https://x/$id",
            publishedAt = Instant.parse("2026-10-23T10:00:00Z"), read = false, starred = false,
        )
    }
}
