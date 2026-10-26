package dev.rubensalas.atalaya.ui.papers

import dev.rubensalas.atalaya.data.api.ApiException
import dev.rubensalas.atalaya.data.api.EntrySummary
import dev.rubensalas.atalaya.data.api.FullPaper
import dev.rubensalas.atalaya.data.api.FullPaperState
import dev.rubensalas.atalaya.data.api.Paper
import dev.rubensalas.atalaya.data.api.PaperField
import dev.rubensalas.atalaya.data.api.PaperPage
import dev.rubensalas.atalaya.data.api.PaperQuery
import dev.rubensalas.atalaya.data.api.PaperSummary
import dev.rubensalas.atalaya.data.api.QueryTranslation
import dev.rubensalas.atalaya.data.api.TranslationState
import dev.rubensalas.atalaya.data.papers.PapersApi
import dev.rubensalas.atalaya.data.papers.PapersRepository
import dev.rubensalas.atalaya.data.papers.RecentSearch
import dev.rubensalas.atalaya.data.papers.RecentSearches
import dev.rubensalas.atalaya.data.papers.addRecent
import dev.rubensalas.atalaya.ui.article.ArticleVersion
import dev.rubensalas.atalaya.ui.article.Block
import dev.rubensalas.atalaya.ui.article.SpanStyleKind
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
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
class PaperViewModelsTest {
    private class FakeApi : PapersApi {
        val translated = mutableListOf<String>()
        val searches = mutableListOf<PaperQuery>()
        val papers = ArrayDeque<Paper>()
        var last: Paper = PAPER
        var saves = 0
        var failSave = false

        override suspend fun translatePaperQuery(q: String): QueryTranslation {
            translated += q
            return QueryTranslation(q, "en: $q")
        }

        override suspend fun searchPapers(query: PaperQuery): PaperPage {
            searches += query
            return PaperPage("en: ${query.q}", 1, listOf(PAPER.summary))
        }

        override suspend fun paper(arxivId: String): Paper {
            last = papers.removeFirstOrNull() ?: last
            return last
        }

        override suspend fun fullPaper(arxivId: String): FullPaper = FullPaper(FullPaperState.NotRequested)

        override suspend fun requestFullPaper(arxivId: String): FullPaper = FullPaper(FullPaperState.NotRequested)

        override suspend fun savePaper(arxivId: String): EntrySummary {
            if (failSave) throw ApiException(null, "No se pudo conectar con el servicio")
            saves++
            return EntrySummary(99, 5, "Papers guardados", PAPER.title, PAPER.originalTitle, true, "en", PAPER.absUrl, PAPER.publishedAt, false, true)
        }
    }

    private class FakeRecents : RecentSearches {
        override val searches = MutableStateFlow<List<RecentSearch>>(emptyList())
        override suspend fun add(search: RecentSearch) {
            searches.value = addRecent(searches.value, search)
        }
        override suspend fun remove(search: RecentSearch) {
            searches.value = searches.value - search
        }
    }

    private val dispatcher = StandardTestDispatcher()
    private val scope = TestScope(dispatcher)
    private val api = FakeApi()
    private var savedCallbacks = 0
    private val repo = PapersRepository(api, onSaved = { savedCallbacks++ })
    private val recents = FakeRecents()

    @Before
    fun setUp() = Dispatchers.setMain(dispatcher)

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun previewWaitsUntilTypingStops() = with(scope) {
        val vm = PaperSearchViewModel(repo, recents, debounceMs = 600)
        runCurrent()
        vm.setText("modelos")
        advanceTimeBy(400)
        vm.setText("modelos pequeños")
        advanceTimeBy(599)
        runCurrent()
        assertTrue(api.translated.isEmpty())
        assertNull(vm.state.value.english)

        advanceTimeBy(2)
        runCurrent()
        assertEquals(listOf("modelos pequeños"), api.translated)
        assertEquals("en: modelos pequeños", vm.state.value.english)
    }

    @Test
    fun authorSearchIsNotTranslated() = with(scope) {
        val vm = PaperSearchViewModel(repo, recents, debounceMs = 600)
        vm.setField(PaperField.Author)
        vm.setText("Ortega")
        advanceTimeBy(1_000)
        runCurrent()
        assertTrue(api.translated.isEmpty())
        assertNull(vm.state.value.english)
    }

    @Test
    fun searchShowsResultsAndRemembersIt() = with(scope) {
        val vm = PaperSearchViewModel(repo, recents, debounceMs = 600)
        vm.setText("  destilación ")
        vm.setField(PaperField.Title)
        vm.search()
        runCurrent()

        val s = vm.state.value
        assertTrue(s.showResults)
        assertFalse(s.searching)
        assertEquals(PaperQuery("destilación", PaperField.Title, limit = 20), api.searches.single())
        assertEquals("en: destilación", s.englishQuery)
        assertEquals(1, s.papers.size)
        assertEquals(listOf(RecentSearch("destilación", PaperField.Title)), s.recents)

        vm.startEditing()
        assertFalse(vm.state.value.showResults)
    }

    @Test
    fun savingAPaperMarksItsResultRow() = with(scope) {
        val search = PaperSearchViewModel(repo, recents, debounceMs = 600)
        search.setText("x")
        search.search()
        runCurrent()
        assertNull(search.state.value.papers.single().savedEntryId)

        val paper = PaperViewModel(PAPER.arxivId, repo, pollInterval = 3_000)
        runCurrent()
        paper.save()
        runCurrent()
        paper.save()
        runCurrent()

        assertEquals(1, api.saves)
        assertEquals(1, savedCallbacks)
        assertTrue(paper.state.value.saved)
        assertEquals(99L, search.state.value.papers.single().savedEntryId)
    }

    @Test
    fun saveFailureShowsMessage() = with(scope) {
        api.failSave = true
        val vm = PaperViewModel(PAPER.arxivId, repo, pollInterval = 3_000)
        runCurrent()
        vm.save()
        runCurrent()
        assertFalse(vm.state.value.saved)
        assertEquals("No se pudo guardar", vm.state.value.message)
    }

    @Test
    fun pendingPaperPollsUntilSpanishArrives() = with(scope) {
        api.papers += PAPER
        api.papers += PAPER
        api.papers += PAPER.copy(translation = TranslationState.Done, translatedAbstract = "Primero.\n\nSegundo.", title = "Destilar")
        val vm = PaperViewModel(PAPER.arxivId, repo, pollInterval = 3_000)
        vm.setActive(true)
        runCurrent()

        var s = vm.state.value
        assertTrue(s.translating)
        assertFalse(s.canSwitch)
        assertEquals(ArticleVersion.Original, s.showing)
        assertEquals(listOf("First."), s.paragraphs)

        advanceTimeBy(3_001)
        runCurrent()
        assertTrue(vm.state.value.translating)

        advanceTimeBy(3_001)
        runCurrent()
        s = vm.state.value
        assertFalse(s.translating)
        assertEquals(ArticleVersion.Spanish, s.showing)
        assertEquals("Destilar", s.title)
        assertEquals(listOf("Primero.", "Segundo."), s.paragraphs)

        vm.choose(ArticleVersion.Original)
        assertEquals(PAPER.originalTitle, vm.state.value.title)
        assertEquals(listOf("First."), vm.state.value.paragraphs)
    }

    @Test
    fun abstractWithFormulasIsShownAsBlocksInEachVersion() = with(scope) {
        api.papers += PAPER.copy(
            translation = TranslationState.Done,
            translatedAbstract = "Sea x.\n\nSegundo.",
            abstractHtml = "<p>Let <img class=\"atalaya-formula\" src=\"/formulas/2610.08812/r1.svg\" alt=\"y\"></p><p>Second.</p>",
            translatedAbstractHtml = "<p>Sea <img class=\"atalaya-formula\" src=\"/formulas/2610.08812/r1.svg\" alt=\"x\"></p><p>Segundo.</p>",
        )
        val vm = PaperViewModel(PAPER.arxivId, repo, serviceUrl = { "https://svc.test" }, parseDispatcher = dispatcher, pollInterval = 3_000)
        runCurrent()

        val spanish = vm.state.value.showingAbstractBlocks!!
        assertEquals(2, spanish.size)
        val formula = (spanish[0] as Block.Paragraph).text.spans.single { it.style is SpanStyleKind.Formula }.style as SpanStyleKind.Formula
        assertEquals("https://svc.test/formulas/2610.08812/r1.svg", formula.url)
        assertEquals("x", formula.latex)

        vm.choose(ArticleVersion.Original)
        val original = vm.state.value.showingAbstractBlocks!!
        assertEquals(2, original.size)
        val originalFormula = (original[0] as Block.Paragraph).text.spans.single { it.style is SpanStyleKind.Formula }.style as SpanStyleKind.Formula
        assertEquals("y", originalFormula.latex)
    }

    @Test
    fun abstractWithoutHtmlStaysPlainText() = with(scope) {
        api.papers += PAPER.copy(translation = TranslationState.Done, translatedAbstract = "Primero.\n\nSegundo.")
        val vm = PaperViewModel(PAPER.arxivId, repo, pollInterval = 3_000)
        runCurrent()

        assertNull(vm.state.value.showingAbstractBlocks)
        assertEquals(listOf("Primero.", "Segundo."), vm.state.value.paragraphs)
        vm.choose(ArticleVersion.Original)
        assertNull(vm.state.value.showingAbstractBlocks)
        assertEquals(listOf("First."), vm.state.value.paragraphs)
    }

    companion object {
        val PAPER = Paper(
            arxivId = "2610.08812",
            title = "Distilling reasoning",
            originalTitle = "Distilling reasoning",
            titleTranslated = false,
            authors = listOf("Wei Chen"),
            publishedAt = Instant.parse("2026-10-21T00:00:00Z"),
            category = "cs.CL",
            abstractText = "First.",
            translation = TranslationState.Pending,
            absUrl = "https://arxiv.org/abs/2610.08812",
            pdfUrl = "https://arxiv.org/pdf/2610.08812",
        )
    }
}
