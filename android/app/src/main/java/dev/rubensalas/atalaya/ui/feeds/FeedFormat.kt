package dev.rubensalas.atalaya.ui.feeds

import dev.rubensalas.atalaya.data.api.Feed
import dev.rubensalas.atalaya.ui.relativeTime
import java.net.URI
import java.time.Instant
import java.util.Locale
import kotlin.math.roundToInt

private val SPANISH = Locale.forLanguageTag("es")

/** `en` → «Inglés». Null si no se conoce el idioma. */
fun languageName(lang: String?): String? {
    if (lang.isNullOrBlank()) return null
    val name = Locale.forLanguageTag(lang).getDisplayLanguage(SPANISH)
    if (name.isBlank() || name.equals(lang, ignoreCase = true)) return lang.uppercase()
    return name.replaceFirstChar { it.titlecase(SPANISH) }
}

/** «unos 30 artículos al día» (F2). Null si el servicio no pudo calcularlo. */
fun perDayText(articlesPerDay: Double?): String? {
    val n = articlesPerDay ?: return null
    return when {
        n < 0.5 -> "menos de un artículo al día"
        n.roundToInt() == 1 -> "un artículo al día"
        else -> "unos ${n.roundToInt()} artículos al día"
    }
}

/** «Inglés · unos 30 artículos al día», con lo que se sepa. */
fun languageAndRate(lang: String?, articlesPerDay: Double?): String =
    listOfNotNull(languageName(lang), perDayText(articlesPerDay)).joinToString(" · ")
        .replaceFirstChar { it.titlecase(SPANISH) }
        .ifEmpty { "Sin datos de idioma ni frecuencia" }

/** `https://www.theverge.com/x` → `theverge.com`. */
fun hostOf(url: String): String {
    val host = runCatching { URI(url.trim()).host }.getOrNull() ?: return url
    return host.removePrefix("www.")
}

/** `https://theverge.com/rss/index.xml` → `theverge.com/rss/index.xml`. */
fun withoutScheme(url: String): String = url.substringAfter("://").removePrefix("www.")

/** Segunda línea de un feed en la lista (pantalla 9). */
data class FeedSubtitle(val text: String, val failing: Boolean)

fun feedSubtitle(feed: Feed, now: Instant = Instant.now()): FeedSubtitle {
    if (feed.error != null) {
        val text = feed.checkedAt?.let { "No se pudo actualizar · ${relativeTime(it, now)}" } ?: "No se pudo actualizar"
        return FeedSubtitle(text, failing = true)
    }
    val host = hostOf(feed.siteUrl.ifBlank { feed.feedUrl })
    return FeedSubtitle(if (feed.crawler) "$host · artículo completo" else host, failing = false)
}
