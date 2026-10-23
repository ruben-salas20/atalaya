package dev.rubensalas.atalaya.ui

import java.time.Duration
import java.time.Instant

/** «hace 5 min», «hace 2 h», «hace 3 días». Lo usan el menú lateral (S2) y los ajustes. */
fun relativeTime(at: Instant, now: Instant = Instant.now()): String {
    val d = Duration.between(at, now).coerceAtLeast(Duration.ZERO)
    return when {
        d.toMinutes() < 1 -> "ahora"
        d.toHours() < 1 -> "hace ${d.toMinutes()} min"
        d.toDays() < 1 -> "hace ${d.toHours()} h"
        d.toDays() == 1L -> "hace 1 día"
        else -> "hace ${d.toDays()} días"
    }
}
