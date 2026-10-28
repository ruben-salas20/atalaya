package dev.rubensalas.atalaya.ui.list

import dev.rubensalas.atalaya.ui.relativeTime
import java.time.Duration
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

private val SPANISH = Locale.forLanguageTag("es")
private val DAY_MONTH = DateTimeFormatter.ofPattern("d MMM", SPANISH)
private val DAY_MONTH_YEAR = DateTimeFormatter.ofPattern("d MMM yyyy", SPANISH)

/** La hora de una fila de la lista (L2): «ahora», «5 min», «2 h», «3 días», «21 oct», «4 mar 2019». */
fun shortAge(at: Instant, now: Instant = Instant.now(), zone: ZoneId = ZoneId.systemDefault()): String {
    val d = Duration.between(at, now).coerceAtLeast(Duration.ZERO)
    return when {
        d.toMinutes() < 1 -> "ahora"
        d.toHours() < 1 -> "${d.toMinutes()} min"
        d.toDays() < 1 -> "${d.toHours()} h"
        d.toDays() == 1L -> "1 día"
        d.toDays() < 7 -> "${d.toDays()} días"
        else -> {
            val date = at.atZone(zone)
            val format = if (date.year == now.atZone(zone).year) DAY_MONTH else DAY_MONTH_YEAR
            // Java escribe «oct.»; en la lista sobra el punto.
            format.format(date).replace(".", "")
        }
    }
}

/**
 * La fecha en la cabecera de un artículo: «hace 3 días» o, si pasó una semana
 * o más, la fecha («4 mar 2019»). Antes salía «hace 727 días» (#24).
 */
fun articleAge(at: Instant, now: Instant = Instant.now(), zone: ZoneId = ZoneId.systemDefault()): String =
    if (Duration.between(at, now).toDays() < 7) relativeTime(at, now)
    else shortAge(at, now, zone)

/** La etiqueta del idioma original (L2): `EN`, o null si es español o aún no se sabe. */
fun langBadge(lang: String?): String? =
    lang?.trim()?.lowercase()?.takeIf { it.isNotEmpty() && it != "es" }?.uppercase()
