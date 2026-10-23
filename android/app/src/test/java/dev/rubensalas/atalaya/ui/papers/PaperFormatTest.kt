package dev.rubensalas.atalaya.ui.papers

import dev.rubensalas.atalaya.data.api.PaperField
import dev.rubensalas.atalaya.data.papers.RecentSearch
import dev.rubensalas.atalaya.data.papers.addRecent
import org.junit.Assert.assertEquals
import org.junit.Test
import java.time.Instant

class PaperFormatTest {
    @Test
    fun shortAuthorsUsesSurnames() {
        assertEquals("", shortAuthors(emptyList()))
        assertEquals("Chen", shortAuthors(listOf("Wei Chen")))
        assertEquals("Nakamura y Raman", shortAuthors(listOf("Kenji Nakamura", "Priya Raman")))
        assertEquals(
            "Chen y otros 4",
            shortAuthors(listOf("Wei Chen", "Lucía Ortega", "Kenji Nakamura", "Priya Raman", "Tom Becker")),
        )
    }

    @Test
    fun paperDateHasYearAndNoDot() {
        assertEquals("21 oct 2026", paperDate(Instant.parse("2026-10-21T23:59:00Z")))
    }

    @Test
    fun resultsHeaderPluralAndThousands() {
        assertEquals("38 resultados · buscado como «small language models»", resultsHeader(38, "small language models"))
        assertEquals("1 resultado · buscado como «x»", resultsHeader(1, "x"))
        assertEquals("12.345 resultados · buscado como «x»", resultsHeader(12_345, "x"))
    }

    @Test
    fun abstractParagraphsSplitOnBlankLines() {
        assertEquals(
            listOf("Primero en dos líneas.", "Segundo."),
            abstractParagraphs("Primero en\ndos líneas.\n\n  \nSegundo.\n"),
        )
        assertEquals(emptyList<String>(), abstractParagraphs(""))
    }

    @Test
    fun recentLabelShowsField() {
        assertEquals("destilación", RecentSearch("destilación").label())
        assertEquals("autor: Ortega", RecentSearch("Ortega", PaperField.Author).label())
    }

    @Test
    fun addRecentMovesRepeatedToTopAndCaps() {
        val list = listOf(RecentSearch("a"), RecentSearch("b"), RecentSearch("b", PaperField.Title))
        assertEquals(
            listOf(RecentSearch("B"), RecentSearch("a"), RecentSearch("b", PaperField.Title)),
            addRecent(list, RecentSearch(" B ")),
        )
        assertEquals(list, addRecent(list, RecentSearch("   ")))
        assertEquals(2, addRecent(list, RecentSearch("c"), max = 2).size)
    }
}
