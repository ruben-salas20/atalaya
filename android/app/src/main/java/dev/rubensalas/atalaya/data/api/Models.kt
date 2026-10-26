package dev.rubensalas.atalaya.data.api

import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import java.time.Instant

// Copia en Kotlin del contrato de crates/atalaya-api/src/lib.rs (ADR 0006).
// Si cambia allí, cambia aquí. Las rutas están en AtalayaClient.

typealias Id = Long

object InstantSerializer : KSerializer<Instant> {
    override val descriptor = PrimitiveSerialDescriptor("Instant", PrimitiveKind.STRING)
    override fun serialize(encoder: Encoder, value: Instant) = encoder.encodeString(value.toString())
    override fun deserialize(decoder: Decoder): Instant = Instant.parse(decoder.decodeString())
}

@Serializable
data class ApiError(val error: String)

@Serializable
data class Health(val status: String, val version: String)

/** `GET /api/status`: lo que usa el botón «Probar» de los ajustes (S3). */
@Serializable
data class Status(val version: String, val miniflux: Boolean)

// ---------------------------------------------------------------------------
// Lectura
// ---------------------------------------------------------------------------

/** Vista fija de la lista (L1). */
@Serializable
enum class View {
    @SerialName("all") All,
    @SerialName("unread") Unread,
    @SerialName("starred") Starred,
}

/**
 * Parámetros de `GET /api/entries`. `view` se combina con `feedId` o
 * `categoryId`; si llegan los dos, manda `feedId`. `limit` vale 50 por
 * defecto y 200 como mucho.
 */
data class EntryQuery(
    val view: View = View.All,
    val feedId: Id? = null,
    val categoryId: Id? = null,
    val limit: Int? = null,
    val offset: Int? = null,
    /** Texto a buscar en el título original o traducido. Vacío o nulo: sin búsqueda. */
    val q: String? = null,
)

/**
 * Parámetros de `POST /api/entries/mark-all-as-read`. Sin ninguno marca
 * todo lo de «Todos». Si llegan los dos, manda `feedId`.
 */
data class MarkAllReadQuery(
    val feedId: Id? = null,
    val categoryId: Id? = null,
)

@Serializable
data class EntryPage(val total: Long, val entries: List<EntrySummary>)

/** Una fila de la lista (L2, L3). */
@Serializable
data class EntrySummary(
    val id: Id,
    @SerialName("feed_id") val feedId: Id,
    @SerialName("feed_title") val feedTitle: String,
    /** En español si ya está traducido; si no, el original. */
    val title: String,
    @SerialName("original_title") val originalTitle: String,
    @SerialName("title_translated") val titleTranslated: Boolean,
    /** ISO 639-1 en minúsculas, o null si aún no se detectó. Se muestra en mayúsculas, salvo `es`. */
    val lang: String? = null,
    val url: String,
    @SerialName("published_at") @Serializable(InstantSerializer::class) val publishedAt: Instant,
    val read: Boolean,
    val starred: Boolean,
)

/** Estado de la traducción del texto completo (L5, L6). */
@Serializable
enum class TranslationState {
    /** Ya está en español, o no se pudo detectar el idioma. */
    @SerialName("not_needed") NotNeeded,
    /** Se muestra el original con «Traduciendo…» y se vuelve a pedir cada pocos segundos. */
    @SerialName("pending") Pending,
    @SerialName("done") Done,
    /** Se muestra el original. */
    @SerialName("failed") Failed,
}

/** Avance de una traducción en curso (ADR 0015): `done` de `total` párrafos. */
@Serializable
data class Progress(val done: Int, val total: Int)

/**
 * `GET /api/entries/{id}`. Pedirlo pone en marcha la traducción del texto
 * completo (ADR 0003). En el JSON, los campos de [EntrySummary] van al mismo
 * nivel que estos.
 */
@Serializable
data class Entry(
    val id: Id,
    @SerialName("feed_id") val feedId: Id,
    @SerialName("feed_title") val feedTitle: String,
    val title: String,
    @SerialName("original_title") val originalTitle: String,
    @SerialName("title_translated") val titleTranslated: Boolean,
    val lang: String? = null,
    val url: String,
    @SerialName("published_at") @Serializable(InstantSerializer::class) val publishedAt: Instant,
    val read: Boolean,
    val starred: Boolean,
    val author: String? = null,
    /** HTML original (L7: el servicio ya pidió el artículo completo si hacía falta). */
    val content: String,
    val translation: TranslationState,
    /**
     * HTML en español. Con [TranslationState.Done], todo; con [TranslationState.Pending],
     * lo traducido hasta ahora y el resto en inglés, en `span.atalaya-pendiente` (ADR 0015).
     */
    @SerialName("translated_content") val translatedContent: String? = null,
    /** Con [TranslationState.Pending], cuántos párrafos van (ADR 0015). Puede faltar. */
    val progress: Progress? = null,
    /** Si es un paper de arXiv, su identificador (`2610.08812`); si no, null. Abre «Texto completo» (#90). */
    @SerialName("arxiv_id") val arxivId: String? = null,
) {
    val summary: EntrySummary
        get() = EntrySummary(id, feedId, feedTitle, title, originalTitle, titleTranslated, lang, url, publishedAt, read, starred)
}

/** `PATCH /api/entries/{id}`. Lo que es null no cambia. Guardar también traduce el texto (T2). */
@Serializable
data class EntryUpdate(val read: Boolean? = null, val starred: Boolean? = null)

/** `GET /api/sidebar`: el menú lateral (L1). */
@Serializable
data class Sidebar(
    val total: Long,
    val unread: Long,
    val starred: Long,
    /** En orden alfabético. */
    val categories: List<SidebarCategory>,
)

@Serializable
data class SidebarCategory(val id: Id, val title: String, val unread: Long, val feeds: List<SidebarFeed>)

@Serializable
data class SidebarFeed(val id: Id, val title: String, val unread: Long)

// ---------------------------------------------------------------------------
// Feeds y carpetas (ADR 0007). En Miniflux, una carpeta es una «categoría».
// ---------------------------------------------------------------------------

@Serializable
data class Category(val id: Id, val title: String)

@Serializable
data class CategoryInput(val title: String)

/** `POST /api/feeds/discover` (F1). Acepta `theverge.com`, sin `https://`. */
@Serializable
data class DiscoverRequest(val url: String)

/** Un feed encontrado en una web, aún sin añadir (F1, F2). */
@Serializable
data class DiscoveredFeed(
    @SerialName("feed_url") val feedUrl: String,
    val title: String,
    val lang: String? = null,
    @SerialName("articles_per_day") val articlesPerDay: Double? = null,
)

/** `POST /api/feeds` (F3). `crawler` es descargar el artículo completo. */
@Serializable
data class NewFeed(
    @SerialName("feed_url") val feedUrl: String,
    @SerialName("category_id") val categoryId: Id,
    val crawler: Boolean,
)

/** `PATCH /api/feeds/{id}` (F4). Lo que es null no cambia. */
@Serializable
data class FeedUpdate(
    val title: String? = null,
    @SerialName("category_id") val categoryId: Id? = null,
    val crawler: Boolean? = null,
)

/** Un feed suscrito (F2, F4, F6). */
@Serializable
data class Feed(
    val id: Id,
    val title: String,
    @SerialName("site_url") val siteUrl: String,
    @SerialName("feed_url") val feedUrl: String,
    val category: Category,
    val crawler: Boolean,
    val lang: String? = null,
    @SerialName("articles_per_day") val articlesPerDay: Double? = null,
    @SerialName("checked_at") @Serializable(InstantSerializer::class) val checkedAt: Instant? = null,
    val error: FeedError? = null,
    /** Si es el feed de los papers guardados. Lo dice el servicio; no se mira la URL. */
    val papers: Boolean = false,
)

/** Fallo al actualizar un feed (F6). `message` es el de Miniflux, tal cual. */
@Serializable
data class FeedError(val message: String, val count: Int)

// ---------------------------------------------------------------------------
// Papers de arXiv (ADR 0005)
// ---------------------------------------------------------------------------

/** `GET /api/papers/translate`: cómo se buscará en arXiv una búsqueda en español (P1). */
@Serializable
data class QueryTranslation(
    /** La búsqueda tal como llegó. */
    val q: String,
    /** La búsqueda en inglés. Igual que `q` si ya estaba en inglés. */
    val english: String,
)

/** Dónde buscar en arXiv (P2). */
@Serializable
enum class PaperField {
    @SerialName("all") All,
    @SerialName("title") Title,
    @SerialName("author") Author,
    @SerialName("abstract") Abstract,
}

/** Cómo ordenar los resultados (P2). */
@Serializable
enum class PaperSort {
    /** Del más nuevo al más viejo, por fecha de envío. */
    @SerialName("recent") Recent,
    @SerialName("relevance") Relevance,
}

/**
 * Parámetros de `GET /api/papers/search`. `q` va en español; con
 * [PaperField.Author] no se traduce. `limit` vale 20 por defecto y 50 como mucho.
 */
data class PaperQuery(
    val q: String,
    val field: PaperField = PaperField.All,
    val sort: PaperSort = PaperSort.Recent,
    val limit: Int? = null,
    val offset: Int? = null,
)

/** Una página de resultados (P1, P3). */
@Serializable
data class PaperPage(
    /** La búsqueda tal como se envió a arXiv, para «buscado como …». */
    @SerialName("english_query") val englishQuery: String,
    /** Cuántos resultados hay en total según arXiv, no solo en la página. */
    val total: Long,
    val papers: List<PaperSummary>,
)

/** Una fila de los resultados (P3). */
@Serializable
data class PaperSummary(
    /** Sin versión: `2610.08812`. Los antiguos llevan barra: `hep-th/9901001`. */
    @SerialName("arxiv_id") val arxivId: String,
    /** En español si ya está traducido; si no, el original. */
    val title: String,
    @SerialName("original_title") val originalTitle: String,
    @SerialName("title_translated") val titleTranslated: Boolean,
    /** En el orden de arXiv. */
    val authors: List<String>,
    /** Fecha de la primera versión. */
    @SerialName("published_at") @Serializable(InstantSerializer::class) val publishedAt: Instant,
    /** Categoría principal de arXiv: `cs.CL`. */
    val category: String,
    /** El artículo de Miniflux si el paper está en Guardados; null si no. */
    @SerialName("saved_entry_id") val savedEntryId: Id? = null,
)

/**
 * `GET /api/papers/{arxiv_id}` (P4). Pedirlo pone en marcha la traducción del
 * resumen, como [Entry]. En el JSON, los campos de [PaperSummary] van al mismo
 * nivel que estos.
 */
@Serializable
data class Paper(
    @SerialName("arxiv_id") val arxivId: String,
    val title: String,
    @SerialName("original_title") val originalTitle: String,
    @SerialName("title_translated") val titleTranslated: Boolean,
    val authors: List<String>,
    @SerialName("published_at") @Serializable(InstantSerializer::class) val publishedAt: Instant,
    val category: String,
    @SerialName("saved_entry_id") val savedEntryId: Id? = null,
    /** Resumen original en texto plano, con los párrafos separados por una línea en blanco. */
    @SerialName("abstract_text") val abstractText: String,
    val translation: TranslationState,
    /** Resumen en español, con los mismos párrafos. Solo con [TranslationState.Done]. */
    @SerialName("translated_abstract") val translatedAbstract: String? = null,
    /**
     * El resumen original en HTML, solo si tiene fórmulas (#80): un `<p>` por
     * párrafo y las fórmulas como en [FullPaper.html]. Sin fórmulas es null y se
     * usa [abstractText].
     */
    @SerialName("abstract_html") val abstractHtml: String? = null,
    /** Lo mismo para [translatedAbstract]. Solo con [TranslationState.Done]. */
    @SerialName("translated_abstract_html") val translatedAbstractHtml: String? = null,
    /** `https://arxiv.org/abs/2610.08812`. */
    @SerialName("abs_url") val absUrl: String,
    /** `https://arxiv.org/pdf/2610.08812`. */
    @SerialName("pdf_url") val pdfUrl: String,
) {
    val summary: PaperSummary
        get() = PaperSummary(arxivId, title, originalTitle, titleTranslated, authors, publishedAt, category, savedEntryId)
}

/** Estado del paper completo traducido (P7, ADR 0010). */
@Serializable
enum class FullPaperState {
    /** Nadie lo ha pedido: la app muestra «Leer completo». */
    @SerialName("not_requested") NotRequested,
    /** arXiv no tiene versión HTML: quedan el resumen y el PDF. */
    @SerialName("not_available") NotAvailable,
    /** Se está traduciendo; `html` trae lo que ya hay (ADR 0015). */
    @SerialName("pending") Pending,
    @SerialName("done") Done,
    /** Falló; pedirlo otra vez lo reintenta. */
    @SerialName("failed") Failed,
}

/**
 * `GET /api/papers/{arxiv_id}/full` y `POST` (lo pide; responde igual). Con
 * `Pending`, `html` trae lo traducido y el resto en el original, dentro de
 * `atalaya-pendiente`. Las fórmulas son imágenes SVG con una ruta relativa
 * (`/formulas/ID/N.svg`), sin clave: la app la resuelve contra su servicio.
 */
@Serializable
data class FullPaper(
    val state: FullPaperState,
    val html: String? = null,
    val progress: Progress? = null,
)
