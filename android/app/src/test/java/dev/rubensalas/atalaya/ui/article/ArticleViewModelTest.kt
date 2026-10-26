package dev.rubensalas.atalaya.ui.article

import dev.rubensalas.atalaya.data.api.ApiException
import dev.rubensalas.atalaya.data.api.Entry
import dev.rubensalas.atalaya.data.api.EntryPage
import dev.rubensalas.atalaya.data.api.EntryQuery
import dev.rubensalas.atalaya.data.api.EntrySummary
import dev.rubensalas.atalaya.data.api.EntryUpdate
import dev.rubensalas.atalaya.data.api.FullPaper
import dev.rubensalas.atalaya.data.api.FullPaperState
import dev.rubensalas.atalaya.data.api.Id
import dev.rubensalas.atalaya.data.api.MarkAllReadQuery
import dev.rubensalas.atalaya.data.api.Paper
import dev.rubensalas.atalaya.data.api.PaperPage
import dev.rubensalas.atalaya.data.api.PaperQuery
import dev.rubensalas.atalaya.data.api.Progress
import dev.rubensalas.atalaya.data.api.QueryTranslation
import dev.rubensalas.atalaya.data.api.Sidebar
import dev.rubensalas.atalaya.data.api.TranslationState
import dev.rubensalas.atalaya.data.list.ListRepository
import dev.rubensalas.atalaya.data.papers.PapersApi
import dev.rubensalas.atalaya.data.papers.PapersRepository
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
        override suspend fun markAllRead(scope: MarkAllReadQuery) = Unit
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

    /** Solo cuenta las lecturas y peticiones del paper completo. */
    private class FakePapersApi : PapersApi {
        val fullReads = mutableListOf<String>()
        val fullRequests = mutableListOf<String>()
        var full = FullPaper(FullPaperState.NotRequested)

        override suspend fun translatePaperQuery(q: String): QueryTranslation = throw UnsupportedOperationException()
        override suspend fun searchPapers(query: PaperQuery): PaperPage = throw UnsupportedOperationException()
        override suspend fun paper(arxivId: String): Paper = throw UnsupportedOperationException()
        override suspend fun savePaper(arxivId: String): EntrySummary = throw UnsupportedOperationException()

        override suspend fun fullPaper(arxivId: String): FullPaper {
            fullReads += arxivId
            return full
        }

        override suspend fun requestFullPaper(arxivId: String): FullPaper {
            fullRequests += arxivId
            return full
        }
    }

    private val dispatcher = StandardTestDispatcher()
    private val scope = TestScope(dispatcher)
    private val repo = FakeRepo()
    private val papersApi = FakePapersApi()
    private val papers = PapersRepository(papersApi)

    @Before
    fun setUp() = Dispatchers.setMain(dispatcher)

    @After
    fun tearDown() = Dispatchers.resetMain()

    private fun newVm() = ArticleViewModel(7, repo, papers, parseDispatcher = dispatcher, pollInterval = 3_000)

    @Test
    fun articleWithoutArxivIdNeverAsksForTheFullText() = with(scope) {
        val vm = newVm()
        runCurrent()
        vm.setActive(true)
        advanceTimeBy(30_000)
        assertTrue(papersApi.fullReads.isEmpty())
        assertTrue(papersApi.fullRequests.isEmpty())
        assertEquals(FullPaperState.NotRequested, vm.full.value.state)
    }

    @Test
    fun paperArticleReadsItsFullTextOnce() = with(scope) {
        repo.responses += ENTRY.copy(arxivId = "2610.08812", translation = TranslationState.NotNeeded)
        val vm = newVm()
        runCurrent()
        assertEquals(listOf("2610.08812"), papersApi.fullReads)

        // Releer el artículo (p. ej. al volver a abrirlo) no vuelve a abrir el paper.
        vm.load()
        runCurrent()
        assertEquals(listOf("2610.08812"), papersApi.fullReads)
    }

    @Test
    fun fullTextPollsWhileShownAndStopsInBackground() = with(scope) {
        papersApi.full = FullPaper(FullPaperState.Pending, html = "<p>Hola</p>", progress = Progress(1, 2))
        repo.responses += ENTRY.copy(arxivId = "2610.08812", translation = TranslationState.NotNeeded)
        val vm = newVm()
        runCurrent()
        assertEquals(1, papersApi.fullReads.size)

        vm.setActive(true)
        advanceTimeBy(3_001)
        assertEquals(2, papersApi.fullReads.size)
        assertEquals(Progress(1, 2), vm.full.value.progress)

        vm.setActive(false)
        advanceTimeBy(30_000)
        assertEquals(2, papersApi.fullReads.size)
    }

    @Test
    fun readFullAsksTheServiceOnce() = with(scope) {
        repo.responses += ENTRY.copy(arxivId = "2610.08812", translation = TranslationState.NotNeeded)
        val vm = newVm()
        runCurrent()
        vm.readFull()
        runCurrent()
        assertEquals(listOf("2610.08812"), papersApi.fullRequests)
        assertEquals(FullPaperState.NotRequested, vm.full.value.state)
        assertFalse(vm.full.value.requesting)
    }

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
    fun partialTranslationShowsSpanishWhileItGrows() = with(scope) {
        val partial = "<p>Hola</p><p><span class=\"atalaya-pendiente\">Two.</span></p>"
        repo.responses += ENTRY.copy(translation = TranslationState.Pending, translatedContent = partial, progress = Progress(1, 2))
        repo.responses += ENTRY.copy(translation = TranslationState.Pending, translatedContent = "<p>Hola</p><p>Dos.</p>", progress = Progress(2, 2))
        repo.responses += ENTRY.copy(translation = TranslationState.Done, translatedContent = "<p>Hola</p><p>Dos.</p>")
        val vm = newVm()
        vm.setActive(true)
        runCurrent()

        var s = vm.state.value
        assertTrue(s.translating)
        assertTrue(s.canSwitch)
        assertEquals(ArticleVersion.Spanish, s.showing)
        assertEquals(Progress(1, 2), s.progress)
        assertEquals(
            listOf(
                Block.Paragraph(RichText("Hola")),
                Block.Paragraph(RichText("Two.", listOf(Span(0, 4, SpanStyleKind.Pending)))),
            ),
            s.blocks,
        )

        // Llega más: se vuelve a leer el HTML aunque el bloque anterior ya estuviera.
        advanceTimeBy(3_001)
        s = vm.state.value
        assertTrue(s.translating)
        assertEquals(Progress(2, 2), s.progress)
        assertEquals(listOf(Block.Paragraph(RichText("Hola")), Block.Paragraph(RichText("Dos."))), s.blocks)

        advanceTimeBy(3_000)
        s = vm.state.value
        assertFalse(s.translating)
        assertNull(s.progress)
        assertEquals(listOf(Block.Paragraph(RichText("Hola")), Block.Paragraph(RichText("Dos."))), s.blocks)
    }

    @Test
    fun progressTextAndFraction() {
        assertEquals("Traduciendo… 12 de 52", translationProgressText(Progress(12, 52)))
        assertEquals(0.5f, translationFraction(Progress(1, 2)), 0f)
        assertEquals(0f, translationFraction(Progress(0, 0)), 0f)
        assertEquals(1f, translationFraction(Progress(7, 5)), 0f)
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
        val vm = ArticleViewModel(7, failing, papers, parseDispatcher = dispatcher)
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
