package dev.rubensalas.atalaya.data.update

import kotlinx.coroutines.test.runTest
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.time.Instant

class UpdateCheckerTest {
    private val server = MockWebServer()
    private lateinit var checker: UpdateChecker

    @Before
    fun setUp() {
        server.start()
        val releases = ReleaseClient(server.url("/repos/x/y/releases/latest").toString())
        checker = UpdateChecker(releases, installed = Version(0, 2, 0))
    }

    @After
    fun tearDown() = server.close()

    private fun releaseJson(tag: String) = """
        {"tag_name":"$tag","body":"- Nuevo","assets":[
          {"name":"atalaya.apk","browser_download_url":"${server.url("/dl/atalaya.apk")}"},
          {"name":"SHA256SUMS","browser_download_url":"${server.url("/dl/SHA256SUMS")}"}
        ]}
    """.trimIndent()

    @Test
    fun newerReleaseIsOffered() = runTest {
        server.enqueue(MockResponse(body = releaseJson("v0.2.1")))
        val result = checker.check()
        assertTrue(result is UpdateCheck.Available)
        result as UpdateCheck.Available
        assertEquals("v0.2.1", result.release.tag)
        assertEquals("- Nuevo", result.release.notes)
        val request = server.takeRequest()
        assertEquals("/repos/x/y/releases/latest", request.url.encodedPath)
        // Repo público: no se manda ningún token.
        assertNull(request.headers["Authorization"])
    }

    @Test
    fun sameOrOlderReleaseIsUpToDate() = runTest {
        server.enqueue(MockResponse(body = releaseJson("v0.2.0")))
        assertEquals(UpdateCheck.UpToDate, checker.check())

        server.enqueue(MockResponse(body = releaseJson("v0.1.9")))
        assertEquals(UpdateCheck.UpToDate, checker.check())
    }

    @Test
    fun serverErrorIsFailedWithText() = runTest {
        server.enqueue(MockResponse(code = 500, body = "{}"))
        val result = checker.check()
        assertTrue(result is UpdateCheck.Failed)
        assertEquals("GitHub respondió 500", (result as UpdateCheck.Failed).message)
    }

    @Test
    fun brokenJsonIsFailed() = runTest {
        server.enqueue(MockResponse(body = "esto no es json"))
        assertTrue(checker.check() is UpdateCheck.Failed)
    }

    @Test
    fun releaseWithoutApkIsFailed() = runTest {
        server.enqueue(MockResponse(body = """{"tag_name":"v0.2.1","assets":[]}"""))
        assertTrue(checker.check() is UpdateCheck.Failed)
    }

    @Test
    fun noConnectionIsFailed() = runTest {
        // Puerto cerrado: no hay nadie escuchando, como sin red.
        val offline = UpdateChecker(ReleaseClient("http://127.0.0.1:1/releases/latest"), Version(0, 2, 0))
        val result = offline.check()
        assertTrue(result is UpdateCheck.Failed)
        assertEquals("No se pudo conectar con GitHub", (result as UpdateCheck.Failed).message)
    }

    @Test
    fun automaticCheckEverySixHours() {
        val last = Instant.parse("2026-10-25T10:00:00Z")
        assertTrue(isAutoCheckDue(null, last))
        assertFalse(isAutoCheckDue(last, last.plusSeconds(5 * 3600 + 59 * 60)))
        assertTrue(isAutoCheckDue(last, last.plusSeconds(6 * 3600)))
        assertTrue(isAutoCheckDue(last, last.plusSeconds(7 * 3600)))
        // Si la hora guardada está en el futuro (el reloj cambió), se comprueba.
        assertTrue(isAutoCheckDue(last, last.minusSeconds(60)))
    }
}
