package dev.rubensalas.atalaya.ui.list

import dev.rubensalas.atalaya.data.api.ApiException
import dev.rubensalas.atalaya.data.api.EntryPage
import dev.rubensalas.atalaya.data.api.EntryQuery
import dev.rubensalas.atalaya.data.api.EntrySummary
import dev.rubensalas.atalaya.data.api.Sidebar
import dev.rubensalas.atalaya.data.api.SidebarCategory
import dev.rubensalas.atalaya.data.api.SidebarFeed
import dev.rubensalas.atalaya.data.api.View
import dev.rubensalas.atalaya.data.list.ListRepository
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
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

        override suspend fun markSynced() {
            synced++
        }
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
