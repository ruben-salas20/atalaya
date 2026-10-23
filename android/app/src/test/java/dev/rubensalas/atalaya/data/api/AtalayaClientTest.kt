package dev.rubensalas.atalaya.data.api

import kotlinx.coroutines.test.runTest
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Test

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
    }

    @Test
    fun entriesQueryAndPatchBody() = runTest {
        server.enqueue(MockResponse(body = """{"total":0,"entries":[]}"""))
        client.entries(EntryQuery(view = View.Unread, categoryId = 3, limit = 20))
        val list = server.takeRequest().url
        assertEquals("/api/entries", list.encodedPath)
        assertEquals("view=unread&category_id=3&limit=20", list.encodedQuery)

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
    fun errorUsesServiceMessage() = runTest {
        server.enqueue(MockResponse(code = 401, body = """{"error":"falta la clave o no es correcta"}"""))
        val e = runCatching { client.status() }.exceptionOrNull() as ApiException
        assertEquals(401, e.status)
        assertEquals("Falta la clave o no es correcta", e.message)
    }
}
