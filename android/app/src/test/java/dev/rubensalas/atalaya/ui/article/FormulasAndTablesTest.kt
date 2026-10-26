package dev.rubensalas.atalaya.ui.article

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

private const val SERVICE = "https://atalaya.rubensalas.dev"
private const val FORMULA_SRC = "https://atalaya.rubensalas.dev/formulas/2610.31646v1/3.svg"
private val INLINE_FORMULA = """<img class="atalaya-formula" alt="\alpha" src="/formulas/2610.31646v1/3.svg" style="vertical-align: -0.439ex; width: 5.429ex; height: 2.5ex">"""

class FormulasAndTablesTest {
    @Test
    fun inlineFormulaKeepsItsSizeAndAlignmentInEx() {
        val p = parseArticleHtml("<p>Sea $INLINE_FORMULA la tasa.</p>", SERVICE).single() as Block.Paragraph
        assertEquals("Sea ￼ la tasa.", p.text.text)
        assertEquals(
            listOf(Span(4, 5, SpanStyleKind.Formula(FORMULA_SRC, FormulaSize(5.429f, 2.5f, -0.439f), "\\alpha"))),
            p.text.spans,
        )
    }

    @Test
    fun formulaWithoutStyleUsesTheDefaultSize() {
        val p = parseArticleHtml("""<p>x <img class="atalaya-formula" src="/formulas/a/1.svg"></p>""", SERVICE).single() as Block.Paragraph
        val formula = p.text.spans.single().style as SpanStyleKind.Formula
        assertEquals(DEFAULT_FORMULA_SIZE, formula.size)
    }

    @Test
    fun blockFormulaIsItsOwnBlockWithSizeInEx() {
        val blocks = parseArticleHtml(
            """<p class="ltx_equation">Antes<img class="atalaya-formula atalaya-formula-bloque" alt="a+b" src="/formulas/a/2.svg" style="width: 8ex; height: 3ex; vertical-align: -1ex"></p>""",
            SERVICE,
        )
        assertEquals(
            listOf(
                Block.Paragraph(RichText("Antes")),
                Block.Formula("$SERVICE/formulas/a/2.svg", FormulaSize(8f, 3f, -1f), "a+b"),
            ),
            blocks,
        )
    }

    @Test
    fun exValuesAreReadAndOtherUnitsAreRejected() {
        assertEquals(5.429f, parseEx("5.429ex")!!, 0.0001f)
        assertEquals(-0.439f, parseEx(" -0.439ex ")!!, 0.0001f)
        assertNull(parseEx("12px"))
        assertNull(parseEx("ex"))
        assertEquals(FormulaSize(5f, 2f, 0f), parseFormulaStyle("width: 5ex; height: 12px"))
    }

    @Test
    fun relativeFormulaUrlIsResolvedAgainstTheServiceAddress() {
        assertEquals(FORMULA_SRC, parseArticleHtml("<p>$INLINE_FORMULA</p>", SERVICE).single().let {
            ((it as Block.Paragraph).text.spans.single().style as SpanStyleKind.Formula).url
        })
        assertEquals(FORMULA_SRC, parseArticleHtml("<p>$INLINE_FORMULA</p>", "$SERVICE/").single().let {
            ((it as Block.Paragraph).text.spans.single().style as SpanStyleKind.Formula).url
        })
    }

    @Test
    fun currentColorTakesTheThemeColorInEveryCase() {
        val svg = """<g fill="currentColor"><path stroke="CurrentColor" d="M0 0"/></g>"""
        assertEquals("""<g fill="#E8E6E3"><path stroke="#E8E6E3" d="M0 0"/></g>""", recolorSvg(svg, "#E8E6E3"))
        assertEquals("""<rect fill="none"/>""", recolorSvg("""<rect fill="none"/>""", "#000000"))
        assertEquals("#112233", svgHexColor(0xFF112233.toInt()))
    }

    @Test
    fun tableBecomesRowsOfCellsWithHeaders() {
        val blocks = parseArticleHtml(
            "<table><tr><th>Modelo</th><th>Acc</th></tr><tr><td>Base <b>M</b></td><td>91.2</td></tr></table>",
            SERVICE,
        )
        assertEquals(
            listOf(
                Block.Table(
                    listOf(
                        listOf(TableCell(RichText("Modelo"), header = true), TableCell(RichText("Acc"), header = true)),
                        listOf(
                            TableCell(RichText("Base M", listOf(Span(5, 6, SpanStyleKind.Bold)))),
                            TableCell(RichText("91.2")),
                        ),
                    ),
                ),
            ),
            blocks,
        )
    }

    @Test
    fun colspanHeaderCoversTwoColumns() {
        val table = parseArticleHtml(
            """<table><tr><th colspan="2">Mejora relativa al original (%)</th><th>Memoria (%)</th></tr>""" +
                """<tr><td>a</td><td>b</td><td>c</td></tr></table>""",
            SERVICE,
        ).single() as Block.Table
        val header = table.rows[0]
        // La cabecera mide 2 + 1 = 3 columnas, igual que la fila de datos de tres celdas.
        assertEquals(3, header.sumOf { it.colspan })
        assertEquals(3, table.rows[1].sumOf { it.colspan })
        assertEquals(2, header[0].colspan)
        assertEquals("Mejora relativa al original (%)", header[0].text.text)
        assertEquals(1, header[1].colspan)
        assertEquals("Memoria (%)", header[1].text.text)
    }

    @Test
    fun svgFigureFromArxivIsAPlainImageNotAFormula() {
        val blocks = parseArticleHtml(
            """<figure class="ltx_figure"><img src="https://arxiv.org/html/2610.31646v1/fig.svg" alt="Figura" class="ltx_graphics ltx_img_landscape"></figure>""",
            SERVICE,
        )
        assertEquals(listOf(Block.Image("https://arxiv.org/html/2610.31646v1/fig.svg", "Figura")), blocks)
    }

    @Test
    fun tableCaptionComesBeforeTheTable() {
        val blocks = parseArticleHtml(
            """<figure class="ltx_table"><figcaption>Tabla 1: resultados</figcaption><table><tr><td>a</td></tr></table></figure>""",
            SERVICE,
        )
        assertEquals(Block.Paragraph(RichText("Tabla 1: resultados")), blocks[0])
        assertEquals(Block.Table(listOf(listOf(TableCell(RichText("a"))))), blocks[1])
        assertEquals(2, blocks.size)
    }

    @Test
    fun nestedTableKeepsItsRowsOutOfTheOuterTable() {
        val blocks = parseArticleHtml(
            "<table><tr><td>A<table><tr><td>x</td></tr></table></td><td>B</td></tr></table>",
            SERVICE,
        )
        // La tabla de dentro no añade filas a la de fuera: la de fuera tiene una fila y dos celdas.
        assertEquals(1, blocks.size)
        val row = (blocks.single() as Block.Table).rows.single()
        assertEquals(2, row.size)
        assertTrue(row[0].text.text.contains("x"))
        assertEquals("B", row[1].text.text)
    }

    @Test
    fun latexFallbackIsShownAsCode() {
        val p = parseArticleHtml("""<p><code class="atalaya-latex">\foo{x}</code></p>""", SERVICE).single() as Block.Paragraph
        assertEquals("\\foo{x}", p.text.text)
        assertEquals(listOf(Span(0, 7, SpanStyleKind.Code)), p.text.spans)
    }
}
