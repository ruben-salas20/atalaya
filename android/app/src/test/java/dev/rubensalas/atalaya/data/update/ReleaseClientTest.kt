package dev.rubensalas.atalaya.data.update

import dev.rubensalas.atalaya.data.api.ApiException
import kotlinx.coroutines.test.runTest
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.fail
import org.junit.Before
import org.junit.Test
import java.io.File
import java.nio.file.Files

class ReleaseClientTest {
    private val server = MockWebServer()
    private lateinit var dir: File

    @Before
    fun setUp() {
        server.start()
        dir = Files.createTempDirectory("release").toFile()
    }

    @After
    fun tearDown() {
        server.close()
        dir.deleteRecursively()
    }

    @Test
    fun downloadWritesTheFile() = runTest {
        server.enqueue(MockResponse(body = "contenido del apk"))
        val target = File(dir, "atalaya.apk")
        ReleaseClient(server.url("/").toString()).download(server.url("/atalaya.apk").toString(), target)
        assertEquals("contenido del apk", target.readText())
    }

    @Test
    fun failedDownloadLeavesNoFile() = runTest {
        server.enqueue(MockResponse(code = 404))
        val target = File(dir, "atalaya.apk")
        try {
            ReleaseClient(server.url("/").toString()).download(server.url("/x").toString(), target)
            fail("debería fallar con 404")
        } catch (e: ApiException) {
            assertEquals(404, e.status)
        }
        assertFalse(target.exists())
    }
}
