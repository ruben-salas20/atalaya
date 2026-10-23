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
    /** HTML en español, solo con [TranslationState.Done]. */
    @SerialName("translated_content") val translatedContent: String? = null,
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
)

/** Fallo al actualizar un feed (F6). `message` es el de Miniflux, tal cual. */
@Serializable
data class FeedError(val message: String, val count: Int)
