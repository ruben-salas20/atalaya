package dev.rubensalas.atalaya.ui.article

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ArticleHtmlTest {
    @Test
    fun paragraphsKeepLinksAndStyles() {
        val blocks = parseArticleHtml(
            "<p>Ahorra en <a href=\"/shop\">los rastreadores</a> con el código <strong>SCARY</strong>.</p>",
            "https://www.theverge.com/news/1",
        )
        val p = blocks.single() as Block.Paragraph
        assertEquals("Ahorra en los rastreadores con el código SCARY.", p.text.text)
        assertEquals(
            listOf(
                Span(10, 26, SpanStyleKind.Link("https://www.theverge.com/shop")),
                Span(41, 46, SpanStyleKind.Bold),
            ),
            p.text.spans.sortedBy { it.start },
        )
    }

    @Test
    fun looseTextBetweenBlocksBecomesAParagraph() {
        val blocks = parseArticleHtml("<h3>Fi Ultra</h3>\nThe Fi Ultra is the first tracker. \n<p>Next.</p>")
        assertEquals(
            listOf(
                Block.Heading(3, RichText("Fi Ultra")),
                Block.Paragraph(RichText("The Fi Ultra is the first tracker.")),
                Block.Paragraph(RichText("Next.")),
            ),
            blocks,
        )
    }

    @Test
    fun figureGivesImageWithCaption() {
        val blocks = parseArticleHtml(
            "<figure><img src=\"https://x/a.png\" alt=\"Collar\"><figcaption> Foto: <em>The Verge</em> </figcaption></figure>",
        )
        val img = blocks.single() as Block.Image
        assertEquals("https://x/a.png", img.url)
        assertEquals("Collar", img.alt)
        assertEquals("Foto: The Verge", img.caption?.text)
    }

    @Test
    fun emptyCaptionAndTrackingPixelsAreDropped() {
        val blocks = parseArticleHtml(
            "<figure><img src=\"https://x/a.png\"><figcaption>\n\t</figcaption></figure><img src=\"https://t/p.gif\" width=\"1\" height=\"1\">",
        )
        assertEquals(listOf(Block.Image("https://x/a.png", "", null)), blocks)
    }

    @Test
    fun listsKeepMarkersAndNesting() {
        val blocks = parseArticleHtml("<ol start=\"3\"><li>Uno</li><li>Dos<ul><li>Sub</li></ul></li></ol><ul><li><p>A</p><p>B</p></li></ul>")
        assertEquals(
            listOf(
                Block.ListItem("3.", 0, RichText("Uno")),
                Block.ListItem("4.", 0, RichText("Dos")),
                Block.ListItem("◦", 1, RichText("Sub")),
                Block.ListItem("•", 0, RichText("A")),
                Block.ListItem("", 0, RichText("B")),
            ),
            blocks,
        )
    }

    @Test
    fun inlineImageSplitsTheParagraph() {
        val blocks = parseArticleHtml("<p>Antes <img src=\"https://x/i.jpg\"> después</p>")
        assertEquals(
            listOf(
                Block.Paragraph(RichText("Antes")),
                Block.Image("https://x/i.jpg", ""),
                Block.Paragraph(RichText("después")),
            ),
            blocks,
        )
    }

    @Test
    fun lineBreaksQuotesAndCode() {
        val blocks = parseArticleHtml("<blockquote><p>Una cita</p></blockquote><p>a<br>b</p><pre>fn main() {\n    x\n}</pre><hr>")
        assertEquals(
            listOf(
                Block.Quote(RichText("Una cita")),
                Block.Paragraph(RichText("a\nb")),
                Block.Code("fn main() {\n    x\n}"),
                Block.Rule,
            ),
            blocks,
        )
    }

    @Test
    fun scriptsAndEmptyParagraphsAreSkipped() {
        val blocks = parseArticleHtml("<script>alert(1)</script><p>&nbsp;</p><p> Texto </p><style>p{}</style>")
        assertEquals(listOf(Block.Paragraph(RichText("Texto"))), blocks)
    }

    @Test
    fun boldSpanningABreakingImageStaysOnBothSides() {
        val blocks = parseArticleHtml("<p><b>uno <img src=\"https://x/i.jpg\"> dos</b></p>")
        val first = blocks[0] as Block.Paragraph
        val last = blocks[2] as Block.Paragraph
        assertEquals(listOf(Span(0, 3, SpanStyleKind.Bold)), first.text.spans)
        assertEquals(listOf(Span(0, 3, SpanStyleKind.Bold)), last.text.spans)
    }

    @Test
    fun embedsBecomeLinks() {
        val blocks = parseArticleHtml("<iframe src=\"https://www.youtube.com/embed/abc\"></iframe>")
        val p = blocks.single() as Block.Paragraph
        assertEquals("Ver el contenido incrustado", p.text.text)
        assertTrue(p.text.spans.single().style is SpanStyleKind.Link)
    }
}
