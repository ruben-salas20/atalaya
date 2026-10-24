package dev.rubensalas.atalaya.data.api

import dev.rubensalas.atalaya.data.papers.PapersApi
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.Json
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody
import okhttp3.RequestBody.Companion.toRequestBody
import java.io.IOException
import java.util.concurrent.TimeUnit

/** Dirección del servicio y clave, tal como están en los ajustes. */
data class ServerConfig(val baseUrl: String, val apiKey: String)

/**
 * Fallo de una llamada. [message] está en español y se puede mostrar tal cual.
 * [status] es el código HTTP, o null si no hubo respuesta (sin red, dirección mala…).
 */
class ApiException(val status: Int?, message: String, cause: Throwable? = null) : Exception(message, cause)

/**
 * Cliente de la API del servicio (`crates/atalaya-api`). Lee la dirección y la
 * clave en cada llamada, para que un cambio en los ajustes valga al momento.
 */
class AtalayaClient(
    private val config: suspend () -> ServerConfig,
    private val http: OkHttpClient = defaultHttpClient(),
) : PapersApi {
    val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
    }

    suspend fun health(): Health = get("health")
    suspend fun status(): Status = get("api/status")

    suspend fun sidebar(): Sidebar = get("api/sidebar")

    suspend fun entries(query: EntryQuery = EntryQuery()): EntryPage =
        get("api/entries") {
            addQueryParameter("view", json.encodeToString(View.serializer(), query.view).trim('"'))
            query.feedId?.let { addQueryParameter("feed_id", it.toString()) }
            query.categoryId?.let { addQueryParameter("category_id", it.toString()) }
            query.limit?.let { addQueryParameter("limit", it.toString()) }
            query.offset?.let { addQueryParameter("offset", it.toString()) }
        }

    suspend fun entry(id: Id): Entry = get("api/entries/$id")

    suspend fun updateEntry(id: Id, update: EntryUpdate): EntrySummary =
        send("PATCH", "api/entries/$id", body(update))

    suspend fun markAllRead(scope: MarkAllReadQuery) {
        execute("POST", "api/entries/mark-all-as-read", null) {
            scope.feedId?.let { addQueryParameter("feed_id", it.toString()) }
            scope.categoryId?.let { addQueryParameter("category_id", it.toString()) }
        }
    }

    suspend fun discoverFeeds(url: String): List<DiscoveredFeed> =
        send("POST", "api/feeds/discover", body(DiscoverRequest(url)))

    suspend fun feeds(): List<Feed> = get("api/feeds")
    suspend fun addFeed(feed: NewFeed): Feed = send("POST", "api/feeds", body(feed))
    suspend fun updateFeed(id: Id, update: FeedUpdate): Feed = send("PATCH", "api/feeds/$id", body(update))
    suspend fun deleteFeed(id: Id) = sendNoContent("DELETE", "api/feeds/$id")

    suspend fun categories(): List<Category> = get("api/categories")
    suspend fun addCategory(title: String): Category = send("POST", "api/categories", body(CategoryInput(title)))
    suspend fun renameCategory(id: Id, title: String): Category =
        send("PATCH", "api/categories/$id", body(CategoryInput(title)))

    /** Responde 409 si la carpeta aún tiene feeds. */
    suspend fun deleteCategory(id: Id) = sendNoContent("DELETE", "api/categories/$id")

    // El identificador de arXiv va como un solo segmento: `addPathSegment`
    // codifica la barra de los antiguos (`hep-th%2F9901001`).

    override suspend fun translatePaperQuery(q: String): QueryTranslation =
        get("api/papers/translate") { addQueryParameter("q", q) }

    override suspend fun searchPapers(query: PaperQuery): PaperPage =
        get("api/papers/search") {
            addQueryParameter("q", query.q)
            addQueryParameter("field", json.encodeToString(PaperField.serializer(), query.field).trim('"'))
            addQueryParameter("sort", json.encodeToString(PaperSort.serializer(), query.sort).trim('"'))
            query.limit?.let { addQueryParameter("limit", it.toString()) }
            query.offset?.let { addQueryParameter("offset", it.toString()) }
        }

    override suspend fun paper(arxivId: String): Paper = get("api/papers") { addPathSegment(arxivId) }

    /** 201 si lo acaba de guardar y 200 si ya estaba: en los dos casos, el artículo de Miniflux. */
    override suspend fun savePaper(arxivId: String): EntrySummary =
        decode(execute("POST", "api/papers", null) { addPathSegment(arxivId).addPathSegment("save") })

    // -----------------------------------------------------------------------

    private inline fun <reified T> body(value: T): RequestBody =
        json.encodeToString(value).toRequestBody(JSON)

    private suspend inline fun <reified T> get(path: String, noinline query: HttpUrl.Builder.() -> Unit = {}): T =
        decode(execute("GET", path, null, query))

    private suspend inline fun <reified T> send(method: String, path: String, body: RequestBody): T =
        decode(execute(method, path, body) {})

    private suspend fun sendNoContent(method: String, path: String) {
        execute(method, path, null) {}
    }

    private inline fun <reified T> decode(text: String): T =
        try {
            json.decodeFromString(text)
        } catch (e: SerializationException) {
            throw ApiException(null, "El servicio respondió algo que la app no entiende", e)
        }

    private suspend fun execute(
        method: String,
        path: String,
        body: RequestBody?,
        query: HttpUrl.Builder.() -> Unit,
    ): String {
        val cfg = config()
        val base = cfg.baseUrl.trim().trimEnd('/').toHttpUrlOrNull()
            ?: throw ApiException(null, "La dirección del servicio no es válida")
        val url = base.newBuilder().addPathSegments(path).apply(query).build()
        val request = Request.Builder()
            .url(url)
            .header("Authorization", "Bearer ${cfg.apiKey.trim()}")
            .method(method, body ?: if (method == "GET" || method == "DELETE") null else ByteArray(0).toRequestBody())
            .build()
        return withContext(Dispatchers.IO) {
            try {
                http.newCall(request).execute().use { response ->
                    val text = response.body.string()
                    if (!response.isSuccessful) {
                        val message = runCatching { json.decodeFromString<ApiError>(text).error }.getOrNull()
                            ?: "El servicio respondió ${response.code}"
                        throw ApiException(response.code, message.replaceFirstChar { it.uppercase() })
                    }
                    text
                }
            } catch (e: IOException) {
                throw ApiException(null, "No se pudo conectar con el servicio", e)
            }
        }
    }

    companion object {
        private val JSON = "application/json".toMediaType()

        fun defaultHttpClient(): OkHttpClient = OkHttpClient.Builder()
            .connectTimeout(10, TimeUnit.SECONDS)
            .readTimeout(30, TimeUnit.SECONDS)
            .build()
    }
}
