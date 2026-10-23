package dev.rubensalas.atalaya.ui.papers

import dev.rubensalas.atalaya.data.api.PaperField
import dev.rubensalas.atalaya.data.api.PaperSort
import dev.rubensalas.atalaya.data.papers.RecentSearch
import java.text.NumberFormat
import java.time.Instant
import java.time.ZoneId
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.util.Locale

private val SPANISH = Locale.forLanguageTag("es")
private val DAY_MONTH_YEAR = DateTimeFormatter.ofPattern("d MMM yyyy", SPANISH)

/** Autores de una fila (P3): «Chen», «Nakamura y Raman», «Chen y otros 4». Va el apellido. */
fun shortAuthors(authors: List<String>): String {
    val last = authors.map { it.trim().substringAfterLast(' ') }.filter { it.isNotEmpty() }
    return when (last.size) {
        0 -> ""
        1 -> last[0]
        2 -> "${last[0]} y ${last[1]}"
        else -> "${last[0]} y otros ${last.size - 1}"
    }
}

/** «21 oct 2026». arXiv fecha en UTC. */
fun paperDate(at: Instant, zone: ZoneId = ZoneOffset.UTC): String =
    DAY_MONTH_YEAR.format(at.atZone(zone)).replace(".", "")

/** «38 resultados · buscado como «small language models»» (pantalla 7). */
fun resultsHeader(total: Long, englishQuery: String): String {
    val n = NumberFormat.getIntegerInstance(SPANISH).format(total)
    val word = if (total == 1L) "resultado" else "resultados"
    return "$n $word · buscado como «$englishQuery»"
}

fun PaperField.label(): String = when (this) {
    PaperField.All -> "Todo"
    PaperField.Title -> "Título"
    PaperField.Author -> "Autor"
    PaperField.Abstract -> "Resumen"
}

fun PaperSort.label(): String = when (this) {
    PaperSort.Recent -> "Más recientes"
    PaperSort.Relevance -> "Relevancia"
}

/** Cómo se ve en la lista de recientes: «destilación», «autor: Ortega». */
fun RecentSearch.label(): String =
    if (field == PaperField.All) q else "${field.label().lowercase()}: $q"

/** Los párrafos de un resumen: van separados por una línea en blanco. */
fun abstractParagraphs(text: String): List<String> =
    text.split(Regex("""\n\s*\n""")).map { it.trim().replace(Regex("""\s*\n\s*"""), " ") }.filter { it.isNotEmpty() }
