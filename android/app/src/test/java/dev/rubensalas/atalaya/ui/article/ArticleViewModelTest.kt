package dev.rubensalas.atalaya.ui.article

import dev.rubensalas.atalaya.data.api.ApiException
import dev.rubensalas.atalaya.data.api.Entry
import dev.rubensalas.atalaya.data.api.EntryPage
import dev.rubensalas.atalaya.data.api.EntryQuery
import dev.rubensalas.atalaya.data.api.EntrySummary
import dev.rubensalas.atalaya.data.api.EntryUpdate
import dev.rubensalas.atalaya.data.api.Id
import dev.rubensalas.atalaya.data.api.Sidebar
import dev.rubensalas.atalaya.data.api.TranslationState
import dev.rubensalas.atalaya.data.list.ListRepository
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runCurrent
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
class ArticleViewModelTest {
    private class FakeRepo : ListRepository {
        val responses = ArrayDeque<Entry>()
        var last: Entry = ENTRY
        var opened = 0
        val updates = mutableListOf<EntryUpdate>()
        var failUpdate = false

        override suspend fun sidebar(): Sidebar = throw UnsupportedOperationException()
        override suspend fun entries(query: EntryQuery): EntryPage = throw UnsupportedOperationException()
        override suspend fun markSynced() = Unit

        override suspend fun entry(id: Id): Entry {
            opened++
            last = responses.removeFirstOrNull() ?: last
            return last
        }

        override suspend fun updateEntry(id: Id, update: EntryUpdate): EntrySummary {
            if (failUpdate) throw ApiException(null, "No se pudo conectar con el servicio")
            updates += update
            last = last.copy(read = update.read ?: last.read, starred = update.starred ?: last.starred)
            return last.summary
        }
    }

    private val dispatcher = StandardTestDispatcher()
    private val scope = TestScope(dispatcher)
    private val repo = FakeRepo()

    @Before
    fun setUp() = Dispatchers.setMain(dispatcher)

    @After
    fun tearDown() = Dispatchers.resetMain()

    private fun newVm() = ArticleViewModel(7, repo, parseDispatcher = dispatcher, pollInterval = 3_000)

    @Test
    fun pendingShowsOriginalAndSwitchesToSpanishWhenDone() = with(scope) {
        repo.responses += ENTRY
        repo.responses += ENTRY
        repo.responses += ENTRY.copy(translation = TranslationState.Done, translatedContent = "<p>Hola</p>")
        val vm = newVm()
        vm.setActive(true)
        runCurrent()

        var s = vm.state.value
        assertTrue(s.translating)
        assertFalse(s.canSwitch)
        assertEquals(ArticleVersion.Original, s.showing)
        assertEquals("Hello", s.title)
        assertEquals(listOf(Block.Paragraph(RichText("Hi"))), s.blocks)

        advanceTimeBy(3_001)
        assertTrue(vm.state.value.translating)
        assertEquals(2, repo.opened)

        advanceTimeBy(3_000)
        s = vm.state.value
        assertFalse(s.translating)
        assertEquals(ArticleVersion.Spanish, s.showing)
        assertEquals("Hola mundo", s.title)
        assertEquals(listOf(Block.Paragraph(RichText("Hola"))), s.blocks)

        // Ya traducido, no se vuelve a preguntar.
        advanceTimeBy(30_000)
        assertEquals(3, repo.opened)

        vm.choose(ArticleVersion.Original)
        assertEquals("Hello", vm.state.value.title)
        assertEquals(listOf(Block.Paragraph(RichText("Hi"))), vm.state.value.blocks)
    }

    @Test
    fun noPollingWhileInBackground() = with(scope) {
        val vm = newVm()
        runCurrent()
        advanceTimeBy(30_000)
        assertEquals(1, repo.opened)
        vm.setActive(true)
        advanceTimeBy(3_001)
        assertEquals(2, repo.opened)
        vm.setActive(false)
        advanceTimeBy(30_000)
        assertEquals(2, repo.opened)
    }

    @Test
    fun openingMarksReadOnce() = with(scope) {
        val vm = newVm()
        runCurrent()
        assertEquals(listOf(EntryUpdate(read = true)), repo.updates)
        assertTrue(vm.state.value.entry!!.read)

        vm.toggleRead()
        runCurrent()
        assertFalse(vm.state.value.entry!!.read)
        vm.load()
        runCurrent()
        assertEquals(listOf(EntryUpdate(read = true), EntryUpdate(read = false)), repo.updates)
        assertFalse(vm.state.value.entry!!.read)
    }

    @Test
    fun alreadyReadIsNotPatched() = with(scope) {
        repo.responses += ENTRY.copy(read = true, translation = TranslationState.NotNeeded)
        val vm = newVm()
        runCurrent()
        assertTrue(repo.updates.isEmpty())
        assertFalse(vm.state.value.showSelector)
    }

    @Test
    fun failedStarIsUndone() = with(scope) {
        val vm = newVm()
        runCurrent()
        repo.failUpdate = true
        vm.toggleStarred()
        assertTrue(vm.state.value.entry!!.starred)
        runCurrent()
        assertFalse(vm.state.value.entry!!.starred)
        assertEquals("No se pudo guardar", vm.state.value.message)
        vm.dismissMessage()
        assertNull(vm.state.value.message)
    }

    @Test
    fun missingEntryExplainsIt() = with(scope) {
        val failing = object : ListRepository by repo {
            override suspend fun entry(id: Id): Entry = throw ApiException(404, "No existe")
        }
        val vm = ArticleViewModel(7, failing, parseDispatcher = dispatcher)
        runCurrent()
        assertEquals("Este artículo ya no existe en Miniflux.", vm.state.value.error)
        assertNull(vm.state.value.entry)
    }

    private companion object {
        val ENTRY = Entry(
            id = 7, feedId = 2, feedTitle = "The Verge", title = "Hola mundo", originalTitle = "Hello",
            titleTranslated = true, lang = "en", url = "https://x/7",
            publishedAt = Instant.parse("2026-10-23T10:00:00Z"), read = false, starred = false,
            author = "Ana", content = "<p>Hi</p>", translation = TranslationState.Pending,
        )
    }
}
