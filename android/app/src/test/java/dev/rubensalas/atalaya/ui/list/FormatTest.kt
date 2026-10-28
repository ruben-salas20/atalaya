package dev.rubensalas.atalaya.ui.list

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import java.time.Duration
import java.time.Instant
import java.time.ZoneOffset

class FormatTest {
    private val now = Instant.parse("2026-10-23T18:00:00Z")
    private fun ago(d: Duration) = shortAge(now - d, now, ZoneOffset.UTC)

    @Test
    fun shortAgeSteps() {
        assertEquals("ahora", ago(Duration.ofSeconds(30)))
        assertEquals("5 min", ago(Duration.ofMinutes(5)))
        assertEquals("2 h", ago(Duration.ofHours(2)))
        assertEquals("1 día", ago(Duration.ofHours(30)))
        assertEquals("3 días", ago(Duration.ofDays(3)))
        assertEquals("1 oct", ago(Duration.ofDays(22)))
        assertEquals("4 mar 2019", shortAge(Instant.parse("2019-03-04T12:00:00Z"), now, ZoneOffset.UTC))
    }

    @Test
    fun articleAgeShowsTheDateWhenOld() {
        assertEquals("hace 3 días", articleAge(now - Duration.ofDays(3), now, ZoneOffset.UTC))
        assertEquals("1 oct", articleAge(now - Duration.ofDays(22), now, ZoneOffset.UTC))
        assertEquals("4 mar 2019", articleAge(Instant.parse("2019-03-04T12:00:00Z"), now, ZoneOffset.UTC))
    }

    @Test
    fun futureDatesCountAsNow() {
        assertEquals("ahora", shortAge(now + Duration.ofHours(1), now, ZoneOffset.UTC))
    }

    @Test
    fun badgeHidesSpanishAndUnknown() {
        assertEquals("EN", langBadge("en"))
        assertNull(langBadge("es"))
        assertNull(langBadge(null))
        assertNull(langBadge(""))
    }

    @Test
    fun syncLabelDropsHace() {
        assertEquals("Sincronizado (5 min)", syncLabel(now - Duration.ofMinutes(5), now))
        assertEquals("Sincronizado (ahora)", syncLabel(now, now))
        assertEquals("Sin sincronizar", syncLabel(null, now))
    }
}
