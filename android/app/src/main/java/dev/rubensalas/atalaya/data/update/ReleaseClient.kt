package dev.rubensalas.atalaya.data.update

import dev.rubensalas.atalaya.data.api.ApiException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import java.io.File
import java.io.IOException
import java.util.concurrent.TimeUnit

/**
 * Habla con GitHub para buscar la última Release y bajar sus archivos. No manda
 * ningún token: el repo es público (ADR 0012). Los textos de error están en español.
 */
class ReleaseClient(
    /** Dirección de `releases/latest`. Sale de `BuildConfig.RELEASES_API`. */
    private val releasesApi: String,
    private val http: OkHttpClient = defaultHttpClient(),
) {
    /** La última Release publicada. Lanza [ApiException] si no se puede leer. */
    suspend fun latest(): Release = withContext(Dispatchers.IO) {
        parseRelease(get(releasesApi))
    }

    /** Baja [url] a [target]. Si falla, borra lo que se hubiera escrito. */
    suspend fun download(url: String, target: File) {
        withContext(Dispatchers.IO) {
            try {
                http.newCall(request(url)).execute().use { response ->
                    if (!response.isSuccessful) {
                        throw ApiException(response.code, "GitHub respondió ${response.code} al bajar la actualización")
                    }
                    target.outputStream().use { out -> response.body.byteStream().copyTo(out) }
                }
            } catch (e: IOException) {
                target.delete()
                throw ApiException(null, "No se pudo conectar con GitHub", e)
            } catch (e: ApiException) {
                target.delete()
                throw e
            }
        }
    }

    private fun get(url: String): String = try {
        http.newCall(request(url)).execute().use { response ->
            val text = response.body.string()
            if (!response.isSuccessful) throw ApiException(response.code, "GitHub respondió ${response.code}")
            text
        }
    } catch (e: IOException) {
        throw ApiException(null, "No se pudo conectar con GitHub", e)
    }

    private fun request(url: String): Request = Request.Builder()
        .url(url)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "Atalaya-Android")
        .build()

    companion object {
        fun defaultHttpClient(): OkHttpClient = OkHttpClient.Builder()
            .connectTimeout(10, TimeUnit.SECONDS)
            .readTimeout(30, TimeUnit.SECONDS)
            .build()
    }
}
