package dev.rubensalas.atalaya.ui.feeds

import dev.rubensalas.atalaya.data.api.Category
import dev.rubensalas.atalaya.data.api.Feed
import dev.rubensalas.atalaya.data.api.FeedError
import dev.rubensalas.atalaya.data.api.FeedUpdate
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.Instant

class FeedsLogicTest {
    private val tech = Category(2, "Tecnología")
    private val all = Category(1, "All")
    private val now = Instant.parse("2026-10-23T12:00:00Z")

    private fun feed(id: Long, title: String, category: Category = tech, crawler: Boolean = false, error: FeedError? = null) =
        Feed(
            id = id,
            title = title,
            siteUrl = "https://www.theverge.com",
            feedUrl = "https://www.theverge.com/rss/index.xml",
            category = category,
            crawler = crawler,
            lang = "en",
            articlesPerDay = 40.5,
            checkedAt = Instant.parse("2026-10-21T12:00:00Z"),
            error = error,
        )

    @Test
    fun languageAndRateLikeTheMockup() {
        assertEquals("Inglés · unos 30 artículos al día", languageAndRate("en", 30.2))
        assertEquals("Español", languageAndRate("es", null))
        assertEquals("Menos de un artículo al día", languageAndRate(null, 0.2))
        assertEquals("un artículo al día", perDayText(1.3))
        assertNull(languageName(null))
    }

    @Test
    fun hostsWithoutWww() {
        assertEquals("theverge.com", hostOf("https://www.theverge.com/x"))
        assertEquals("theverge.com/rss/index.xml", withoutScheme("https://www.theverge.com/rss/index.xml"))
    }

    @Test
    fun subtitleShowsCrawlerOrFailure() {
        assertEquals(FeedSubtitle("theverge.com · artículo completo", false), feedSubtitle(feed(1, "V", crawler = true), now))
        assertEquals(
            FeedSubtitle("No se pudo actualizar · hace 2 días", true),
            feedSubtitle(feed(1, "V", error = FeedError("timeout", 3)), now),
        )
    }

    @Test
    fun groupsKeepEmptyFoldersAndSortByTitle() {
        val groups = FeedsViewModel.group(listOf(tech, all), listOf(feed(3, "xataka"), feed(2, "Ars"), feed(1, "PG", all)))
        assertEquals(listOf("All", "Tecnología"), groups.map { it.category.title })
        assertEquals(listOf("Ars", "xataka"), groups[1].feeds.map { it.title })

        val empty = FeedsViewModel.group(listOf(tech, all), listOf(feed(1, "PG", all)))
        assertTrue(empty[1].feeds.isEmpty())
    }

    @Test
    fun editSendsOnlyWhatChanged() {
        val f = feed(1, "The Verge", crawler = true)
        val base = EditFeedUiState(loading = false, feed = f, title = f.title, categoryId = f.category.id, crawler = f.crawler)
        assertNull(base.update)
        assertEquals(FeedUpdate(title = "Verge"), base.copy(title = " Verge ").update)
        assertEquals(FeedUpdate(categoryId = 1, crawler = false), base.copy(categoryId = 1, crawler = false).update)
    }
}
