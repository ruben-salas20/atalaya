package dev.rubensalas.atalaya.data.api

import kotlinx.coroutines.test.runTest
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Test
import java.util.concurrent.TimeUnit

class AtalayaClientTest {
    private val server = MockWebServer()
    private lateinit var client: AtalayaClient

    @Before
    fun setUp() {
        server.start()
        client = AtalayaClient(config = { ServerConfig(server.url("/").toString(), "secreta") })
    }

    @After
    fun tearDown() = server.close()

    @Test
    fun statusSendsKey() = runTest {
        server.enqueue(MockResponse(body = """{"version":"0.1.0","miniflux":true}"""))
        val status = client.status()
        assertEquals(Status("0.1.0", true), status)
        val req = server.takeRequest()
        assertEquals("/api/status", req.url.encodedPath)
        assertEquals("Bearer secreta", req.headers["Authorization"])
    }

    @Test
    fun entryParsesFlattenedSummary() = runTest {
        server.enqueue(
            MockResponse(
                body = """{"id":7,"feed_id":2,"feed_title":"The Verge","title":"Hola","original_title":"Hello",
                "title_translated":true,"lang":"en","url":"https://x","published_at":"2026-10-23T10:00:00Z",
                "read":false,"starred":true,"author":null,"content":"<p>Hello</p>","translation":"pending",
                "translated_content":null,"extra":1}""",
            ),
        )
        val entry = client.entry(7)
        assertEquals(TranslationState.Pending, entry.translation)
        assertEquals("The Verge", entry.summary.feedTitle)
        assertNull(entry.translatedContent)
        assertNull(entry.progress)
    }

    @Test
    fun entryReadsProgressWhenThePartialIsThere() = runTest {
        server.enqueue(
            MockResponse(
                body = """{"id":7,"feed_id":2,"feed_title":"The Verge","title":"Hola","original_title":"Hello",
                "title_translated":true,"lang":"en","url":"https://x","published_at":"2026-10-23T10:00:00Z",
                "read":false,"starred":false,"author":null,"content":"<p>Hello</p>","translation":"pending",
                "translated_content":"<p>Hola</p><p><span class=\"atalaya-pendiente\">Two.</span></p>",
                "progress":{"done":12,"total":52}}""",
            ),
        )
        val entry = client.entry(7)
        assertEquals(TranslationState.Pending, entry.translation)
        assertEquals(Progress(12, 52), entry.progress)
        assertEquals("<p>Hola</p><p><span class=\"atalaya-pendiente\">Two.</span></p>", entry.translatedContent)
    }

    @Test
    fun entriesQueryAndPatchBody() = runTest {
        server.enqueue(MockResponse(body = """{"total":0,"entries":[]}"""))
        client.entries(EntryQuery(view = View.Unread, categoryId = 3, limit = 20))
        val list = server.takeRequest().url
        assertEquals("/api/entries", list.encodedPath)
        assertEquals("view=unread&category_id=3&limit=20", list.encodedQuery)

        server.enqueue(MockResponse(body = """{"total":0,"entries":[]}"""))
        client.entries(EntryQuery(view = View.All, limit = 5, q = "Año nuevo"))
        assertEquals("view=all&limit=5&q=A%C3%B1o%20nuevo", server.takeRequest().url.encodedQuery)

        server.enqueue(
            MockResponse(
                body = """{"id":7,"feed_id":2,"feed_title":"F","title":"T","original_title":"T",
                "title_translated":false,"lang":null,"url":"u","published_at":"2026-10-23T10:00:00Z",
                "read":true,"starred":false}""",
            ),
        )
        client.updateEntry(7, EntryUpdate(read = true))
        val patch = server.takeRequest()
        assertEquals("PATCH", patch.method)
        assertEquals("""{"read":true}""", patch.body?.utf8())
    }

    @Test
    fun translatePaperQuery() = runTest {
        server.enqueue(MockResponse(body = """{"q":"modelos pequeños","english":"small models"}"""))
        val t = client.translatePaperQuery("modelos pequeños")
        assertEquals(QueryTranslation("modelos pequeños", "small models"), t)
        val url = server.takeRequest().url
        assertEquals("/api/papers/translate", url.encodedPath)
        assertEquals("modelos pequeños", url.queryParameter("q"))
    }

    @Test
    fun searchPapersQueryAndPage() = runTest {
        server.enqueue(
            MockResponse(
                body = """{"english_query":"small language models","total":38,"papers":[
                {"arxiv_id":"2610.08812","title":"Destilar el razonamiento","original_title":"Distilling reasoning",
                "title_translated":true,"authors":["Wei Chen","Lucía Ortega"],"published_at":"2026-10-21T17:59:59Z",
                "category":"cs.CL","saved_entry_id":null},
                {"arxiv_id":"hep-th/9901001","title":"Old","original_title":"Old","title_translated":false,
                "authors":[],"published_at":"1999-01-01T00:00:00Z","category":"hep-th","saved_entry_id":42}]}""",
            ),
        )
        val page = client.searchPapers(PaperQuery("modelos", field = PaperField.Abstract, sort = PaperSort.Relevance, limit = 20, offset = 40))
        assertEquals("small language models", page.englishQuery)
        assertEquals(38L, page.total)
        assertEquals(listOf("Wei Chen", "Lucía Ortega"), page.papers[0].authors)
        assertNull(page.papers[0].savedEntryId)
        assertEquals(42L, page.papers[1].savedEntryId)
        assertEquals(java.time.Instant.parse("2026-10-21T17:59:59Z"), page.papers[0].publishedAt)
        val url = server.takeRequest().url
        assertEquals("/api/papers/search", url.encodedPath)
        assertEquals("q=modelos&field=abstract&sort=relevance&limit=20&offset=40", url.encodedQuery)
    }

    @Test
    fun paperParsesFlattenedSummaryAndEncodesOldId() = runTest {
        server.enqueue(
            MockResponse(
                body = """{"arxiv_id":"hep-th/9901001","title":"Título","original_title":"Title","title_translated":true,
                "authors":["A. Uno"],"published_at":"1999-01-01T00:00:00Z","category":"hep-th","saved_entry_id":null,
                "abstract_text":"First.\n\nSecond.","translation":"done","translated_abstract":"Primero.\n\nSegundo.",
                "abs_url":"https://arxiv.org/abs/hep-th/9901001","pdf_url":"https://arxiv.org/pdf/hep-th/9901001"}""",
            ),
        )
        val paper = client.paper("hep-th/9901001")
        assertEquals(TranslationState.Done, paper.translation)
        assertEquals("Primero.\n\nSegundo.", paper.translatedAbstract)
        assertEquals("hep-th", paper.summary.category)
        assertEquals("https://arxiv.org/pdf/hep-th/9901001", paper.pdfUrl)
        assertEquals("/api/papers/hep-th%2F9901001", server.takeRequest().url.encodedPath)
    }

    @Test
    fun paperPendingWithoutTranslation() = runTest {
        server.enqueue(
            MockResponse(
                body = """{"arxiv_id":"2610.08812","title":"Distilling","original_title":"Distilling","title_translated":false,
                "authors":[],"published_at":"2026-10-21T00:00:00Z","category":"cs.CL","saved_entry_id":9,
                "abstract_text":"Text","translation":"pending","translated_abstract":null,
                "abs_url":"https://arxiv.org/abs/2610.08812","pdf_url":"https://arxiv.org/pdf/2610.08812"}""",
            ),
        )
        val paper = client.paper("2610.08812")
        assertEquals(TranslationState.Pending, paper.translation)
        assertNull(paper.translatedAbstract)
        assertEquals(9L, paper.savedEntryId)
    }

    @Test
    fun paperReadsAbstractHtmlWhenItHasFormulas() = runTest {
        server.enqueue(
            MockResponse(
                body = """{"arxiv_id":"2610.08812","title":"Destilar","original_title":"Distilling","title_translated":true,
                "authors":[],"published_at":"2026-10-21T00:00:00Z","category":"cs.CL","saved_entry_id":null,
                "abstract_text":"Sea x.","translation":"done","translated_abstract":"Sea x.",
                "abstract_html":"<p>Sea <img class=\"atalaya-formula\" src=\"/formulas/2610.08812/r1.svg\" alt=\"x\"></p>",
                "translated_abstract_html":"<p>Sea <img class=\"atalaya-formula\" src=\"/formulas/2610.08812/r1.svg\" alt=\"x\"></p>",
                "abs_url":"https://arxiv.org/abs/2610.08812","pdf_url":"https://arxiv.org/pdf/2610.08812"}""",
            ),
        )
        val paper = client.paper("2610.08812")
        assertEquals("<p>Sea <img class=\"atalaya-formula\" src=\"/formulas/2610.08812/r1.svg\" alt=\"x\"></p>", paper.abstractHtml)
        assertEquals(paper.abstractHtml, paper.translatedAbstractHtml)
    }

    @Test
    fun paperWithoutAbstractHtmlKeepsPlainText() = runTest {
        server.enqueue(
            MockResponse(
                body = """{"arxiv_id":"2610.08812","title":"Distilling","original_title":"Distilling","title_translated":false,
                "authors":[],"published_at":"2026-10-21T00:00:00Z","category":"cs.CL","saved_entry_id":null,
                "abstract_text":"First.\n\nSecond.","translation":"not_needed",
                "abs_url":"https://arxiv.org/abs/2610.08812","pdf_url":"https://arxiv.org/pdf/2610.08812"}""",
            ),
        )
        val paper = client.paper("2610.08812")
        assertNull(paper.abstractHtml)
        assertNull(paper.translatedAbstractHtml)
        assertEquals("First.\n\nSecond.", paper.abstractText)
    }

    @Test
    fun fullPaperReadsStateAndProgress() = runTest {
        server.enqueue(MockResponse(body = """{"state":"pending","html":"<p>Hola</p>","progress":{"done":2,"total":9}}"""))
        val full = client.fullPaper("2610.31646")
        assertEquals(FullPaperState.Pending, full.state)
        assertEquals("<p>Hola</p>", full.html)
        assertEquals(Progress(2, 9), full.progress)
        val req = server.takeRequest()
        assertEquals("GET", req.method)
        assertEquals("/api/papers/2610.31646/full", req.url.encodedPath)
    }

    @Test
    fun fullPaperWithoutHtmlAndStatesInSnakeCase() = runTest {
        server.enqueue(MockResponse(body = """{"state":"not_available"}"""))
        assertEquals(FullPaper(FullPaperState.NotAvailable), client.fullPaper("2610.31646"))
        server.enqueue(MockResponse(body = """{"state":"not_requested"}"""))
        assertEquals(FullPaperState.NotRequested, client.fullPaper("2610.31646").state)
    }

    @Test
    fun requestFullPaperPostsAndReturnsState() = runTest {
        server.enqueue(MockResponse(code = 202, body = """{"state":"failed"}"""))
        assertEquals(FullPaperState.Failed, client.requestFullPaper("hep-th/9901001").state)
        val req = server.takeRequest()
        assertEquals("POST", req.method)
        assertEquals("/api/papers/hep-th%2F9901001/full", req.url.encodedPath)
    }

    @Test
    fun savePaperPostsAndReturnsEntry() = runTest {
        server.enqueue(
            MockResponse(
                code = 201,
                body = """{"id":99,"feed_id":5,"feed_title":"Papers guardados","title":"Destilar","original_title":"Distilling",
                "title_translated":true,"lang":"en","url":"https://arxiv.org/abs/hep-th/9901001",
                "published_at":"2026-10-21T00:00:00Z","read":false,"starred":true}""",
            ),
        )
        val entry = client.savePaper("hep-th/9901001")
        assertEquals(99L, entry.id)
        val req = server.takeRequest()
        assertEquals("POST", req.method)
        assertEquals("/api/papers/hep-th%2F9901001/save", req.url.encodedPath)
    }

    @Test
    fun errorUsesServiceMessage() = runTest {
        server.enqueue(MockResponse(code = 401, body = """{"error":"falta la clave o no es correcta"}"""))
        val e = runCatching { client.status() }.exceptionOrNull() as ApiException
        assertEquals(401, e.status)
        assertEquals("Falta la clave o no es correcta", e.message)
    }

    @Test
    fun addFeedReturnsNullWhenTheServiceIsSlow() = runTest {
        // #48: Miniflux puede tardar minutos en crear un feed con «artículo completo».
        val slow = AtalayaClient(
            config = { ServerConfig(server.url("/").toString(), "secreta") },
            addFeedTimeoutSeconds = 1,
        )
        server.enqueue(MockResponse.Builder().body("{}").headersDelay(3, TimeUnit.SECONDS).build())
        assertNull(slow.addFeed(NewFeed(feedUrl = "https://x/feed", categoryId = 1, crawler = true)))
        assertEquals("/api/feeds", server.takeRequest().url.encodedPath)
    }
}
